import assert from "node:assert/strict";
import test from "node:test";
import {
	classicSnapshot as parseSnapshot,
	planClassicUpdate,
} from "./classic-import.ts";

const classicSnapshot = (raw, origin = "fixture") => parseSnapshot(raw, origin);

const header = {
	type: "session",
	version: 3,
	id: "9267f005-48a0-4c42-920e-3277aa2c6070",
	cwd: "/old/home/project",
	timestamp: "2026-10-17T00:00:00Z",
};
const entry = (id, parentId, extra = {}) => ({
	type: "message",
	id,
	parentId,
	timestamp: header.timestamp,
	message: { role: "user", content: id, timestamp: 1 },
	...extra,
});
const text = (...rows) => `${rows.map(JSON.stringify).join("\n")}\n`;
const snap = (...rows) => classicSnapshot(text(header, ...rows));

test("untrusted ancestry diagnostics cannot inject terminal controls", () => {
	const terminalEscape = String.fromCharCode(27);
	assert.throws(
		() => snap(entry("a", `${terminalEscape}]52;c;AAAA`)),
		(error) => !error.message.includes(terminalEscape),
	);
});

test("classic preflight preserves branches and unknown records without executing or dropping them", () => {
	const a = entry("a", null);
	const b = entry("b", "a");
	const c = entry("c", "a", {
		type: "future_extension",
		data: { bytes: "unchanged" },
	});
	const s = snap(a, b, c);
	assert.deepEqual(s.records, [a, b, c]);
	assert.deepEqual(s.leaves, ["b", "c"]);
	assert.equal(s.activeLeaf, "c");
	assert.equal(s.sourceId, `pi:fixture:${header.id}`);
	assert.equal(s.sourceText, text(header, a, b, c));
	assert.equal(s.header.cwd, header.cwd);
});
test("reimport is idempotent under JSON whitespace/key formatting changes", () => {
	const s = snap(entry("a", null));
	const reformatted = classicSnapshot(
		`${[
			JSON.stringify(header, null, 0),
			JSON.stringify(
				Object.fromEntries(Object.entries(s.records[0]).reverse()),
			),
		].join("\n")}\n`,
	);
	assert.equal(planClassicUpdate(s, reformatted, false).kind, "unchanged");
});
test("append update uses stable IDs, never counts or text equality", () => {
	const a = entry("a", null);
	const s = snap(a);
	const updated = snap(a, entry("b", "a"));
	assert.deepEqual(planClassicUpdate(s, updated, false), {
		kind: "append",
		newIds: ["b"],
	});
	assert.equal(planClassicUpdate(s, updated, true).kind, "branch");
	const conflict = snap(
		entry("a", null, { message: { role: "user", content: "different" } }),
	);
	assert.equal(planClassicUpdate(s, conflict, false).kind, "conflict");
	assert.equal(planClassicUpdate(updated, s, false).kind, "conflict");
});
test("new imported branch does not silently replace the selected execution lineage", () => {
	const a = entry("a", null);
	const b = entry("b", "a");
	const s = snap(a, b);
	const updated = snap(a, b, entry("c", "a"));
	assert.equal(planClassicUpdate(s, updated, false).kind, "branch");
});
test("invalid, duplicate and unresolved identities fail closed", () => {
	for (const raw of [
		text({ ...header, version: 2 }),
		text({ ...header, id: "tmp:bad" }),
		text(header, entry("a", null), entry("a", null)),
		text(header, entry("a", "missing")),
		text(header, entry("a", "a")),
		text(header, { ...entry("a", null), parentId: undefined }),
		text(header, header),
	]) {
		assert.throws(() => classicSnapshot(raw));
	}
});
test("partial final lines and corrupt complete lines fail closed rather than silently importing less history", () => {
	assert.throws(
		() => classicSnapshot(`${text(header)}{"type":"message"`),
		/incomplete|JSON/i,
	);
	assert.throws(() => classicSnapshot(`${text(header)}not-json\n`), /JSON/i);
	assert.throws(
		() => classicSnapshot(text(header, entry("a", null)).slice(0, -1)),
		/incomplete/i,
	);
});
test("source changes do not authorize cross-session reconciliation", () => {
	const a = snap(entry("a", null));
	const b = classicSnapshot(
		text(
			{ ...header, id: "9267f005-48a0-4c42-920e-3277aa2c6071" },
			entry("a", null),
		),
	);
	assert.equal(planClassicUpdate(a, b, false).kind, "conflict");
	const otherOrigin = classicSnapshot(
		text(header, entry("a", null)),
		"other-machine",
	);
	assert.equal(planClassicUpdate(a, otherOrigin, false).kind, "conflict");
	assert.throws(() => classicSnapshot(text(header), "../not-an-origin"));
});
