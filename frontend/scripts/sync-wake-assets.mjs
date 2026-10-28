import { cpSync, mkdirSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// ONNX Runtime WASM is served from the app origin like the Moonshine runtime:
// no CDN, one reviewable asset folder. The JS side is bundled by Vite.
const from = fileURLToPath(new URL("../node_modules/onnxruntime-web/dist/", import.meta.url));
const to = fileURLToPath(new URL("../public/speech/ort/", import.meta.url));
mkdirSync(to, { recursive: true });
const wanted = readdirSync(from).filter((name) => /^ort-wasm-simd-threaded(\.[a-z]+)?\.(wasm|mjs)$/.test(name));
if (!wanted.some((name) => name.endsWith(".wasm"))) throw new Error("onnxruntime-web dist has no wasm build");
for (const name of wanted) cpSync(path.join(from, name), path.join(to, name));
console.log(`Staged ${wanted.length} ONNX Runtime asset(s) into public/speech/ort/`);
