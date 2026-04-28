#!/usr/bin/env node

import { copyFile, cp, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const frontendRoot = resolve(__dirname, "..");

const copyFiles = [
	{
		source: "node_modules/mermaid/dist/mermaid.min.js",
		dest: "public/visual-runtime/vendor/mermaid.min.js",
	},
	{
		source: "node_modules/chart.js/dist/chart.umd.min.js",
		dest: "public/visual-runtime/vendor/chart.umd.min.js",
	},
	{
		source: "node_modules/morphdom/dist/morphdom-umd.min.js",
		dest: "public/visual-runtime/vendor/morphdom-umd.min.js",
	},
	{
		source:
			"node_modules/@mermaid-js/layout-elk/dist/mermaid-layout-elk.esm.min.mjs",
		dest: "public/visual-runtime/vendor/mermaid-layout-elk.esm.min.mjs",
	},
];

const copyDirs = [
	{
		source: "node_modules/@mermaid-js/layout-elk/dist/chunks",
		dest: "public/visual-runtime/vendor/chunks",
	},
];

for (const { source, dest } of copyFiles) {
	const src = resolve(frontendRoot, source);
	const out = resolve(frontendRoot, dest);
	await mkdir(dirname(out), { recursive: true });
	await copyFile(src, out);
}

for (const { source, dest } of copyDirs) {
	const src = resolve(frontendRoot, source);
	const out = resolve(frontendRoot, dest);
	await mkdir(dirname(out), { recursive: true });
	await cp(src, out, { recursive: true, force: true });
}

console.log(
	`[visual-runtime] Synced ${copyFiles.length} files and ${copyDirs.length} directory trees.`,
);
