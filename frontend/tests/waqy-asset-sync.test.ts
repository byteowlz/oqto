// The vendored waqy release must be hash-verified; a tampered file must be refused.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { afterEach, describe, expect, test } from "vitest";

const sha = (b: Buffer) => createHash("sha256").update(b).digest("hex");
const temporary: string[] = [];
afterEach(() => {
	for (const dir of temporary.splice(0))
		rmSync(dir, { recursive: true, force: true });
});

// Match the Phonon staging tests: Vite rewrites import.meta.url for browser consumers.
function stageRelease(options: {
	models: string;
	release: string;
	target: string;
	sourceCommit?: string;
}) {
	const script = pathToFileURL(
		path.resolve("scripts/sync-waqy-runtime-assets.mjs"),
	).href;
	return JSON.parse(
		execFileSync(
			"bun",
			[
				"--eval",
				`
		import { syncRelease } from ${JSON.stringify(script)};
		console.log(JSON.stringify(syncRelease(${JSON.stringify(options)})));
	`,
			],
			{ encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
		),
	);
}

function fakeRelease() {
	const root = mkdtempSync(path.join(tmpdir(), "waqy-release-"));
	temporary.push(root);
	const dir = path.join(root, "rel1");
	mkdirSync(dir, { recursive: true });
	const files: Record<string, { file: string; sha256: string; bytes: number }> =
		{};
	for (const [key, name] of [
		["mel", "mel.onnx"],
		["embedding", "emb.onnx"],
		["hey_oqto", "hey.onnx"],
	] as const) {
		const bytes = Buffer.from(`fake-${name}`);
		writeFileSync(path.join(dir, name), bytes);
		files[key] = { file: name, sha256: sha(bytes), bytes: bytes.length };
	}
	writeFileSync(
		path.join(dir, "manifest.json"),
		JSON.stringify({
			release: "rel1",
			threshold: 0.5,
			classifiers: ["hey_oqto"],
			files,
		}),
	);
	return {
		models: root,
		release: "rel1",
		dir,
		target: path.join(root, "output"),
	};
}

describe("sync-waqy-runtime-assets", () => {
	test("copies a verified release and records provenance", () => {
		const { models, release, target } = fakeRelease();
		const provenance = stageRelease({
			models,
			release,
			target,
			sourceCommit: "abc1234",
		});
		expect(provenance.release).toBe("rel1");
		expect(provenance.classifiers).toEqual(["hey_oqto"]);
		expect(readFileSync(path.join(target, "hey.onnx"), "utf8")).toBe(
			"fake-hey.onnx",
		);
		const written = JSON.parse(
			readFileSync(path.join(target, "provenance.json"), "utf8"),
		);
		expect(written.sourceCommit).toBe("abc1234");
		expect(written.files.hey_oqto.sha256).toHaveLength(64);
	});

	test("refuses a tampered file", () => {
		const { models, release, dir, target } = fakeRelease();
		writeFileSync(path.join(dir, "hey.onnx"), "tampered");
		mkdirSync(target);
		writeFileSync(path.join(target, "keep.txt"), "previous verified output");
		expect(() => stageRelease({ models, release, target })).toThrow(
			/checksum mismatch for hey\.onnx/,
		);
		expect(readFileSync(path.join(target, "keep.txt"), "utf8")).toBe(
			"previous verified output",
		);
	});

	test("refuses a manifest without the front end", () => {
		const { models, release, dir, target } = fakeRelease();
		const manifest = JSON.parse(
			readFileSync(path.join(dir, "manifest.json"), "utf8"),
		);
		manifest.files.embedding = undefined;
		writeFileSync(path.join(dir, "manifest.json"), JSON.stringify(manifest));
		expect(() => stageRelease({ models, release, target })).toThrow(
			/mel and embedding/,
		);
	});
});
