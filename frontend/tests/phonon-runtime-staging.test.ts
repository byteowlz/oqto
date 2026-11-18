import { execFileSync } from "node:child_process";
import {
	cpSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { afterEach, expect, it } from "vitest";

const temporary: string[] = [];
const installed = path.resolve("node_modules/phonon-tts");
function temp() {
	const dir = mkdtempSync(path.join(tmpdir(), "oqto-phonon-stage-"));
	temporary.push(dir);
	return dir;
}
// Execute the build script as real ESM: Vite rewrites import.meta.url for browser consumers.
function run(code: string) {
	return execFileSync(
		"bun",
		[
			"--eval",
			`
		import assert from "node:assert/strict";
		import { readFileSync } from "node:fs";
		import { stagePhononRuntime, verifyPhononRuntime } from ${JSON.stringify(pathToFileURL(path.resolve("scripts/sync-phonon-runtime-assets.mjs")).href)};
		const installed = ${JSON.stringify(installed)};
		const lock = readFileSync("bun.lock", "utf8");
		${code}
	`,
		],
		{ encoding: "utf8" },
	);
}
afterEach(() => {
	for (const dir of temporary.splice(0))
		rmSync(dir, { recursive: true, force: true });
});
it("checks exact npm integrity/file hashes and stages the intact worker/WASM tree and licenses", () => {
	const destination = temp();
	run(
		`verifyPhononRuntime(installed, lock); stagePhononRuntime(installed, ${JSON.stringify(destination)});`,
	);
	for (const file of [
		"worker.js",
		"fetch.js",
		"wasm/phonon_tts_bg.wasm",
		"wasm/phonon_tts_simd_bg.wasm",
		"LICENSE-MIT",
		"LICENSE-APACHE",
	]) {
		expect(
			readFileSync(path.join(destination, file)).equals(
				readFileSync(path.join(installed, file)),
			),
		).toBe(true);
	}
	expect(
		JSON.parse(
			readFileSync(path.join(destination, "runtime-manifest.json"), "utf8"),
		).version,
	).toBe("0.4.0");
});
it("refuses changed runtime bytes or missing npm integrity before staging", () => {
	const source = temp();
	cpSync(installed, source, { recursive: true });
	writeFileSync(path.join(source, "worker.js"), "// changed");
	run(`
		assert.throws(() => verifyPhononRuntime(installed, "wrong lock"), /integrity/);
		assert.throws(() => verifyPhononRuntime(${JSON.stringify(source)}, lock), /worker.js/);
	`);
});
