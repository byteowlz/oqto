import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
	chmod,
	lstat,
	mkdir,
	mkdtemp,
	readFile,
	rm,
	symlink,
	writeFile,
} from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import { importClassicSource } from "./classic-client.ts";
import { discoverClassicSources, readClassicSource } from "./classic-source.ts";

const header = {
	type: "session",
	version: 3,
	id: "9267f005-48a0-4c42-920e-3277aa2c6070",
	cwd: "/other-machine/work",
	timestamp: "2026-10-17T00:00:00Z",
};
const raw = `${JSON.stringify(header)}\n${JSON.stringify({
	type: "message",
	id: "a",
	parentId: null,
	timestamp: header.timestamp,
	message: { role: "user", content: "archived", timestamp: 1 },
})}\n`;

test("classic discovery is bounded, read-only and does not traverse symlink sources", async () => {
	const dir = await mkdtemp("/var/tmp/oqt-classic-discover-");
	try {
		const root = join(dir, "sessions");
		await mkdir(join(root, "project"), { recursive: true });
		await writeFile(join(root, "project", "good.jsonl"), raw);
		await writeFile(join(root, "bad.jsonl"), "partial");
		await symlink(
			join(root, "project", "good.jsonl"),
			join(root, "alias.jsonl"),
		);
		const result = await discoverClassicSources(root, "macbook");
		assert.equal(result.sources.length, 1);
		assert.equal(result.sources[0].sessionId, header.id);
		assert.equal(result.sources[0].cwd, header.cwd);
		assert.deepEqual(result.sources[0].leaves, ["a"]);
		assert.ok(result.skipped.length >= 1);
		assert.equal(
			(await discoverClassicSources(join(dir, "missing"), "macbook")).sources
				.length,
			0,
		);
		await assert.rejects(lstat(join(dir, "missing")), /ENOENT/);
	} finally {
		await rm(dir, { recursive: true, force: true });
	}
});

test("explicit classic source is frozen without writing, repairing or following a source symlink", async () => {
	const dir = await mkdtemp("/var/tmp/oqt-classic-source-");
	try {
		const file = join(dir, "source.jsonl");
		await writeFile(file, raw, { mode: 0o400 });
		const before = await lstat(file, { bigint: true });
		const result = await readClassicSource(file, "macbook");
		assert.equal(result.text, raw);
		assert.equal(
			result.sourceHash,
			createHash("sha256").update(raw).digest("hex"),
		);
		assert.equal(result.snapshot.sourceId, `pi:macbook:${header.id}`);
		await assert.rejects(
			importClassicSource("/never-connect", file, "macbook", {
				expectedHash: "0".repeat(64),
			}),
			/changed since discovery/,
		);
		const after = await lstat(file, { bigint: true });
		for (const key of ["ino", "size", "mtimeNs", "ctimeNs", "mode"])
			assert.equal(after[key], before[key]);
		await symlink(file, join(dir, "alias.jsonl"));
		await assert.rejects(
			readClassicSource(join(dir, "alias.jsonl"), "macbook"),
		);
		await chmod(file, 0o600);
		await writeFile(file, raw.trimEnd());
		await assert.rejects(
			readClassicSource(file, "macbook"),
			/newline|Incomplete/,
		);
		assert.equal(
			await readFile(file, "utf8"),
			raw.trimEnd(),
			"reader must never repair a classic source",
		);
		await writeFile(
			file,
			Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from(raw)]),
		);
		await assert.rejects(readClassicSource(file, "macbook"), /Invalid JSON/);
		await writeFile(file, Buffer.from([0xff, 0x0a]));
		await assert.rejects(
			readClassicSource(file, "macbook"),
			/encoded data|encoding/,
		);
	} finally {
		await rm(dir, { recursive: true, force: true });
	}
});
