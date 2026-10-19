import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import {
	parseClassicResumeArgs,
	uploadClassicSource,
} from "./classic-client.ts";
import { serve } from "./guard-wire.ts";

test("classic resume requires explicit origin and parses quoted paths without evaluation", () => {
	assert.deepEqual(
		parseClassicResumeArgs(
			'--classic "/source with spaces.jsonl" --origin macbook --leaf a',
		),
		{ path: "/source with spaces.jsonl", origin: "macbook", leafId: "a" },
	);
	assert.deepEqual(parseClassicResumeArgs("--classic --origin macbook"), {
		origin: "macbook",
	});
	assert.deepEqual(
		parseClassicResumeArgs(
			String.raw`--classic /path\ with\ spaces --origin macbook`,
		),
		{ path: "/path with spaces", origin: "macbook" },
	);
	assert.throws(
		() => parseClassicResumeArgs('--classic "" --origin macbook'),
		/empty/,
	);
	assert.throws(
		() => parseClassicResumeArgs("--classic /source.jsonl"),
		/origin/,
	);
	assert.throws(
		() => parseClassicResumeArgs("--classic --origin a --origin b"),
		/Duplicate/,
	);
	assert.throws(
		() => parseClassicResumeArgs('--classic "unfinished --origin macbook'),
		/quote/,
	);
});

test(
	"classic upload crosses real bounded operator frames without exposing original path",
	{ timeout: 5000 },
	async () => {
		const dir = await mkdtemp("/var/tmp/oqt-classic-upload-");
		const path = join(dir, "operator.sock");
		const bytes = Buffer.from("private-native-archive".repeat(6000));
		const sourceHash = createHash("sha256").update(bytes).digest("hex");
		const chunks = [];
		const peers = new Set();
		let offset = 0;
		const server = await serve(
			path,
			async (method, params) => {
				assert.equal(JSON.stringify(params).includes("original.jsonl"), false);
				if (method === "import_classic_begin") {
					assert.equal(params.sourceHash, sourceHash);
					assert.equal(params.byteLength, bytes.length);
					assert.equal(params.origin, "macbook");
					return { uploadId: "opaque-upload" };
				}
				assert.equal(params.uploadId, "opaque-upload");
				if (method === "import_classic_chunk") {
					assert.equal(params.offset, offset);
					const part = Buffer.from(params.base64, "base64");
					assert.ok(part.length > 0 && part.length <= 16 * 1024);
					assert.ok(Buffer.byteLength(JSON.stringify(params)) < 64 * 1024);
					chunks.push(part);
					offset += part.length;
					return { offset };
				}
				assert.equal(method, "import_classic_commit");
				assert.deepEqual(Buffer.concat(chunks), bytes);
				return {
					sessionId: `classic-${"a".repeat(48)}`,
					sourceHash,
					status: "imported",
					mode: "new_snapshot",
				};
			},
			{ onConnect: (peer) => peers.add(peer) },
		);
		try {
			const result = await uploadClassicSource(
				path,
				{ path: "/never-send/original.jsonl", bytes, sourceHash },
				"macbook",
			);
			assert.equal(result.status, "imported");
			assert.equal(result.sessionId, `classic-${"a".repeat(48)}`);
			assert.equal(result.sourceHash, sourceHash);
			assert.ok(chunks.length > 1);
		} finally {
			for (const peer of peers) peer.socket.destroy();
			await new Promise((resolve) => server.close(resolve));
			await rm(dir, { recursive: true, force: true });
		}
	},
);
