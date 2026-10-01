// Bun-only opt-in proof against real pinned runtime assets, served unchanged by Vite.
// Usage: bun scripts/catalog-proof.mjs <vite-entry.js> <runtime-directory> <chromium>
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { MOONSHINE_RUNTIME } from "./stage-runtime.mjs";
import { mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { MOONSHINE_MODEL_BASES } from "../src/moonshine.ts";

const [viteEntry, runtimeDirectory, chromium] = process.argv.slice(2);
if (!viteEntry || !runtimeDirectory || !chromium)
	throw new Error("Usage: bun scripts/catalog-proof.mjs <vite-entry.js> <runtime-directory> <chromium>");
const digest = createHash("sha256")
	.update(new Uint8Array(await Bun.file(path.join(runtimeDirectory, "moonshine.wasm")).arrayBuffer())).digest("hex");
assert.equal(digest, MOONSHINE_RUNTIME.wasmSha256, "Runtime WASM does not match pinned release");
const { createServer } = await import(pathToFileURL(path.resolve(viteEntry)).href);
const directory = mkdtempSync(path.join(os.tmpdir(), "ears-catalog-proof-"));
const server = await createServer({
	configFile: false,
	root: directory,
	publicDir: path.resolve(runtimeDirectory),
	server: {
		host: "127.0.0.1", port: 5197, strictPort: true,
		watch: { ignored: ["**/profile/**"] },
		headers: {
			"Cross-Origin-Opener-Policy": "same-origin",
			"Cross-Origin-Embedder-Policy": "require-corp",
		},
	},
});
let chrome;
let socket;
const deadline = setTimeout(() => { console.error("Catalog proof timed out"); process.exit(1); }, 60000);
try {
	await server.listen();
	const port = server.httpServer.address().port;
	console.log(`Vite runtime: http://127.0.0.1:${port}`);
	chrome = Bun.spawn([
		path.resolve(chromium), "--headless", "--no-sandbox",
		`--user-data-dir=${directory}/profile`, "--remote-debugging-port=0", "about:blank",
	], { stdout: "ignore", stderr: "ignore" });
	let debugPort;
	for (let i = 0; i < 100; i++) {
		try {
			debugPort = Number((await Bun.file(path.join(directory, "profile/DevToolsActivePort")).text()).split("\n")[0]);
			break;
		} catch { await Bun.sleep(100); }
	}
	assert.ok(debugPort, "Chromium did not expose CDP");
	console.log(`Chromium CDP: ${debugPort}`);
	const tabs = await (await fetch(`http://127.0.0.1:${debugPort}/json`)).json();
	socket = new WebSocket(tabs.find(tab => tab.type === "page").webSocketDebuggerUrl);
	await new Promise((resolve, reject) => {
		socket.addEventListener("open", resolve, { once: true });
		socket.addEventListener("error", reject, { once: true });
	});
	let id = 0;
	const pending = new Map();
	socket.onmessage = event => {
		const message = JSON.parse(event.data);
		if (!message.id) return;
		const promise = pending.get(message.id);
		pending.delete(message.id);
		if (message.error) promise.reject(message.error);
		else promise.resolve(message.result);
	};
	const call = (method, params = {}) => new Promise((resolve, reject) => {
		const next = ++id;
		pending.set(next, { resolve, reject });
		socket.send(JSON.stringify({ id: next, method, params }));
	});
	await call("Page.navigate", { url: `http://127.0.0.1:${port}/module.js` });
	await Bun.sleep(1000);
	const result = await call("Runtime.evaluate", {
		expression: `(async () => {
			const { loadMoonshineModule } = await import('/module.js');
			const module = await loadMoonshineModule();
			return { isolated: crossOriginIsolated,
				en: module.sttDependencies('en', '2', false),
				de: module.sttDependencies('de', '2', false) };
		})()`,
		awaitPromise: true, returnByValue: true,
	});
	assert.equal(result.exceptionDetails, undefined, JSON.stringify(result.exceptionDetails));
	const value = result.result.value;
	assert.equal(value.isolated, true);
	const filenames = ["adapter.ort", "cross_kv.ort", "decoder_kv.ort", "encoder.ort",
		"frontend.model.ort", "frontend.weights.ort", "streaming_config.json", "tokenizer.bin"].sort();
	for (const language of ["en", "de"]) {
		const manifest = JSON.parse(value[language]);
		const base = MOONSHINE_MODEL_BASES[language].replace(/\/$/, "");
		assert.equal(manifest.groups.length, 1);
		const group = manifest.groups[0];
		assert.equal(group.base_url, base);
		assert.deepEqual(group.files.map(file => file.name).sort(), filenames);
		for (const file of group.files) {
			assert.equal(file.url, `${base}/${file.name}`);
			assert.ok(Number.isInteger(file.size) && file.size > 0);
		}
		console.log(JSON.stringify({ language, base, files: group.files,
			total: group.files.reduce((sum, file) => sum + file.size, 0), isolated: true }));
	}
	console.log("PASS: WASM catalog matches both SDK model directories and all eight files");
} finally {
	clearTimeout(deadline);
	socket?.close();
	chrome?.kill();
	if (chrome) await chrome.exited;
	await server.close();
	rmSync(directory, { recursive: true, force: true });
}
