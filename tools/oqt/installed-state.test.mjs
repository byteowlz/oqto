// Installed state and catalog identity. Ports installed_state_test.py to the
// implementation that actually runs at launch (installed-state.ts, ADR-0072).
import assert from "node:assert/strict";
import {
	lstatSync,
	mkdirSync,
	mkdtempSync,
	rmSync,
	statSync,
	symlinkSync,
	unlinkSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { ipcAlias, prepareState, profileKey } from "./installed-state.ts";

const ARTIFACT = "ab".repeat(32);
let root;
let home;

before(() => {
	root = mkdtempSync(join(tmpdir(), "oqt-installed-state-"));
	home = join(root, "state");
	mkdirSync(home, { mode: 0o700 });
	mkdirSync(join(root, "workspace"));
	mkdirSync(join(root, "owner"));
	// /tmp is a realistic shared, sticky IPC parent for the alias tests.
});

after(() => rmSync(root, { recursive: true, force: true }));

const workspace = () => join(root, "workspace");
const owner = () => join(root, "owner");
/** Each test owns its state namespace; the alias key derives from it. */
function freshHome(name) {
	const path = join(root, `state-${name}`);
	mkdirSync(path, { mode: 0o700 });
	return path;
}
const prepare = (overrides = {}) =>
	prepareState({
		artifact: ARTIFACT,
		workdir: workspace(),
		mode: "coding",
		owner: owner(),
		stateHome: home,
		ipcParent: tmpdir(),
		...overrides,
	});

test("catalog identity matches the Python implementation that wrote existing state", () => {
	// Golden values computed with profile_namespace.py; a change here would
	// repartition state and hide every existing Session.
	mkdirSync("/var/tmp/oqt-state-fixture/ws", { recursive: true });
	mkdirSync("/var/tmp/oqt-state-fixture/owner", { recursive: true });
	assert.equal(
		profileKey(
			"/var/tmp/oqt-state-fixture/ws",
			"coding",
			"/var/tmp/oqt-state-fixture/owner",
		),
		"76d0c85e9fe5",
	);
	assert.equal(
		profileKey("/var/tmp/oqt-state-fixture/ws", "demo", null),
		"b8ed78fa876a",
	);
	assert.throws(
		() => profileKey("/var/tmp/oqt-state-fixture/ws", "coding", null),
		/owner profile/,
	);
});

test("reopening is idempotent and the alias is a persistent symlink", () => {
	const first = prepare();
	assert.deepEqual(prepare(), first);
	assert.ok(lstatSync(first.runtimeRoot).isSymbolicLink());
	assert.ok(prepare({ owner: symlinkOwner() }).profile === first.profile);
});

function symlinkOwner() {
	const alias = join(root, "owner-alias");
	if (statSync(alias, { throwIfNoEntry: false }) === undefined)
		symlinkSync(owner(), alias);
	return alias;
}

test("a different owner or state home partitions state separately", () => {
	const first = prepare();
	const otherOwner = join(root, "other-owner");
	mkdirSync(otherOwner, { recursive: true });
	assert.notEqual(prepare({ owner: otherOwner }).profile, first.profile);
	const otherState = join(root, "other-state");
	const second = prepare({ stateHome: otherState });
	assert.notEqual(second.runtimeRoot, first.runtimeRoot);
	assert.ok(statSync(second.profile).isDirectory());
	assert.deepEqual(prepare(), first);
});

test("an existing namespace keeps its permissions and unknown files", () => {
	const stateHome = freshHome("preserve");
	const namespace = join(stateHome, "oqt");
	mkdirSync(namespace, { mode: 0o755 });
	const unknown = join(namespace, "operator-owned");
	writeFileSync(unknown, "preserve");
	prepare({ stateHome });
	assert.equal(statSync(namespace).mode & 0o777, 0o755);
	assert.equal(statSync(unknown).size, 8);
});

test("a colliding IPC alias fails closed", () => {
	const stateHome = freshHome("collision");
	prepare({ stateHome });
	const alias = ipcAlias(stateHome, ARTIFACT, tmpdir());
	unlinkSync(alias);
	symlinkSync(workspace(), alias);
	assert.throws(() => prepare({ stateHome }), /IPC alias/);
});

test("a symlinked or public state namespace fails closed", () => {
	const state2 = join(root, "state-symlinked");
	mkdirSync(state2, { mode: 0o700 });
	symlinkSync(workspace(), join(state2, "oqt"));
	assert.throws(() => prepare({ stateHome: state2 }), /state|safe/i);
	const state3 = join(root, "state-public");
	mkdirSync(join(state3, "oqt", "runtimes"), { recursive: true, mode: 0o777 });
	// mkdir honours umask, so force the permissive mode the check must reject.
	statSync(join(state3, "oqt", "runtimes"));
	assert.throws(() => prepare({ stateHome: state3 }), /safe/);
});

test("the work-directory binding is never written through a symlink", () => {
	const stateHome = freshHome("binding");
	const prepared = prepare({ stateHome });
	const binding = join(prepared.profile, "work-directory.json");
	unlinkSync(binding);
	const protectedFile = join(root, "must-not-write");
	writeFileSync(protectedFile, "untouched");
	symlinkSync(protectedFile, binding);
	assert.throws(() => prepare({ stateHome }), /binding/);
	assert.equal(statSync(protectedFile).size, 9);
});

test("an invalid artifact identity is rejected before any state is created", () => {
	const state4 = join(root, "state-invalid");
	assert.throws(
		() => prepare({ artifact: "../escape", stateHome: state4 }),
		/identity/,
	);
	assert.equal(
		statSync(join(state4, "oqt"), { throwIfNoEntry: false }),
		undefined,
	);
});
