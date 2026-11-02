/** Installed-runtime state, in the launcher's own language (ADR-0072).
 *
 * Ports installed-state.py + profile_namespace.py. The profile key must stay
 * byte-identical to the Python implementation: it names the state partition, so
 * a different key silently hides every existing Session.
 *
 * No preparation lock: every step below is an idempotent atomic syscall
 * (mkdir/symlink/open O_EXCL) and every EEXIST path re-validates what it found.
 * The lock only ever serialised two concurrent first-launches of the same
 * workspace, which these operations already tolerate.
 */
import { createHash } from "node:crypto";
import {
	closeSync,
	lstatSync,
	mkdirSync,
	openSync,
	readFileSync,
	readlinkSync,
	realpathSync,
	statSync,
	symlinkSync,
	writeSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { getuid } from "node:process";

export type Mode = "coding" | "demo";

export class StateError extends Error {}

const fail = (reason: string): never => {
	throw new StateError(reason);
};

export function profileKey(
	workspace: string,
	mode: Mode,
	owner: string | null,
): string {
	const canonicalOwner =
		mode === "coding"
			? requireDirectory(
					owner ?? fail("Coding catalog requires an explicit owner profile"),
					"Owner profile",
				)
			: null;
	const value = [
		requireDirectory(workspace, "Work directory"),
		mode,
		canonicalOwner,
	];
	return createHash("sha256")
		.update(JSON.stringify(value), "utf8")
		.digest("hex")
		.slice(0, 12);
}

function requireDirectory(path: string, label: string): string {
	let resolved: string;
	try {
		resolved = realpathSync(path);
	} catch {
		return fail(`${label} must exist`);
	}
	if (!statSync(resolved).isDirectory())
		return fail(`${label} must be a directory`);
	return resolved;
}

/** Up to the leaf: create missing components 0700 and reject any ancestor that
 * another user owns or can write (sticky shared dirs like /tmp are fine). */
function sharedAncestors(path: string): void {
	const uid = getuid();
	for (const component of ancestors(path)) {
		let meta: ReturnType<typeof lstatSync>;
		try {
			meta = lstatSync(component);
		} catch {
			mkdirSync(component, { mode: 0o700 });
			meta = lstatSync(component);
		}
		if (!meta.isDirectory()) fail(`Not a real directory: ${component}`);
		if (meta.uid !== 0 && meta.uid !== uid)
			fail(`Untrusted directory owner: ${component}`);
		if ((meta.mode & 0o022) !== 0 && (meta.mode & 0o1000) === 0)
			fail(`Writable ancestor: ${component}`);
	}
}

/** A managed directory: owned by us, and either exactly 0700 or merely not
 * group/world writable, matching the installer's distinction. */
function privateDirectory(path: string, strict = true): string {
	sharedAncestors(path);
	const meta = lstatSync(path);
	if (
		meta.uid !== getuid() ||
		(meta.mode & 0o022) !== 0 ||
		(strict && (meta.mode & 0o777) !== 0o700)
	)
		fail(`Not an owned safe directory: ${path}`);
	return path;
}

function ancestors(path: string): string[] {
	const chain: string[] = [];
	for (let current = path; ; current = dirname(current)) {
		chain.unshift(current);
		if (dirname(current) === current) break;
	}
	return chain;
}

/** /var/tmp is shared and sticky; it must be root- or self-owned. */
function requireSharedParent(path: string): void {
	const meta = lstatSync(path);
	if (!meta.isDirectory() || (meta.uid !== 0 && meta.uid !== getuid()))
		fail("Unsafe IPC parent");
	if ((meta.mode & 0o022) !== 0 && (meta.mode & 0o1000) === 0)
		fail("Unsafe IPC parent");
}

/** Short /var/tmp/qi<uid>-<key> symlink to the (possibly long) state dir so
 * confined control sockets stay under the 108-byte Unix limit. */
export function ipcAlias(
	stateHome: string,
	artifact: string,
	parent = "/var/tmp",
): string {
	const target = join(realpathSync(stateHome), "oqt", "runtimes", artifact);
	const key = createHash("sha256")
		.update(target, "utf8")
		.digest("hex")
		.slice(0, 10);
	return join(parent, `qi${getuid()}-${key}`);
}

export interface PreparedState {
	readonly runtimeRoot: string;
	readonly profile: string;
	readonly agent: string;
	readonly home: string;
}

export function prepareState(options: {
	readonly artifact: string;
	readonly workdir: string;
	readonly mode: Mode;
	readonly owner: string | null;
	readonly stateHome: string;
	readonly ipcParent?: string;
}): PreparedState {
	const { artifact, workdir, mode, owner, stateHome } = options;
	if (!/^[a-f0-9]{20,64}$/.test(artifact))
		fail("Invalid installed artifact identity");
	if (!stateHome.startsWith("/")) fail("XDG_STATE_HOME must be absolute");
	// The XDG parent is user-owned shared state, not necessarily mode 0700.
	const root = privateDirectory(stateHome, false);
	const store = privateDirectory(join(root, "oqt"), false);
	privateDirectory(join(store, "runtimes"));
	const state = privateDirectory(join(store, "runtimes", artifact));
	const ipcParent = options.ipcParent ?? "/var/tmp";
	requireSharedParent(ipcParent);
	const alias = ipcAlias(root, artifact, ipcParent);
	try {
		symlinkSync(state, alias, "dir");
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
		const meta = lstatSync(alias);
		if (
			!meta.isSymbolicLink() ||
			meta.uid !== getuid() ||
			readlinkSync(alias) !== state
		)
			fail("Unsafe or colliding IPC alias");
	}
	privateDirectory(join(state, "agent-profile"));
	privateDirectory(join(state, "home"));
	const key = profileKey(workdir, mode, owner);
	privateDirectory(join(state, "p", key));
	bindWorkdir(join(state, "p", key, "work-directory.json"), workdir);
	const shortProfile = join(alias, "p", key);
	if (
		Buffer.byteLength(
			join(shortProfile, "control-00000000-0000-0000-0000-000000000000.sock"),
		) >= 108
	)
		fail("Installed profile exceeds Unix socket limit");
	return {
		runtimeRoot: alias,
		profile: shortProfile,
		agent: join(alias, "agent-profile"),
		home: join(alias, "home"),
	};
}

/** Written once per profile, never rewritten; a changed workspace in an existing
 * profile is a hard error rather than a silent re-point. */
function bindWorkdir(file: string, workdir: string): void {
	const wanted = JSON.stringify({ work_directory: workdir });
	let descriptor: number;
	try {
		descriptor = openSync(file, "wx", 0o600);
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
		const meta = lstatSync(file);
		if (!meta.isFile() || meta.uid !== getuid() || (meta.mode & 0o077) !== 0)
			fail("Invalid work-directory binding");
		let existing: unknown;
		try {
			existing = JSON.parse(readFileSync(file, "utf8"));
		} catch {
			fail("Invalid work-directory binding");
		}
		if (JSON.stringify(existing) !== wanted)
			fail("Invalid work-directory binding");
		return;
	}
	try {
		writeSync(descriptor, wanted);
	} finally {
		closeSync(descriptor);
	}
}
