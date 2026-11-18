// Read-only source lint: Git's index is the inventory, never a directory walk.
import { spawnSync } from "node:child_process";
import { lstatSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const BIN = fileURLToPath(new URL("../node_modules/.bin/", import.meta.url));
const MAX_ARGUMENT_BYTES = 32 * 1024;

export function trackedFiles(cwd) {
	const result = spawnSync("git", ["ls-files", "--cached", "-z", "--", "."], {
		cwd,
		encoding: "utf8",
		maxBuffer: 16 * 1024 * 1024,
	});
	if (result.error) throw result.error;
	if (result.status !== 0)
		throw new Error(`Cannot inventory tracked source: ${result.stderr}`);
	const files = [...new Set(result.stdout.split("\0").filter(Boolean))];
	for (const file of files) {
		// A submodule/directory argument would make the tools walk uncontrolled
		// contents again. Missing tracked files also fail instead of disappearing.
		if (lstatSync(path.join(cwd, file)).isDirectory())
			throw new Error(
				`Tracked directory cannot be linted as a source file: ${file}`,
			);
	}
	return files;
}

function batches(files) {
	const chunks = [];
	let chunk = [];
	let bytes = 0;
	for (const file of files) {
		const argument = `./${file}`;
		const size = Buffer.byteLength(argument) + 1;
		if (chunk.length && bytes + size > MAX_ARGUMENT_BYTES) {
			chunks.push(chunk);
			chunk = [];
			bytes = 0;
		}
		chunk.push(argument);
		bytes += size;
	}
	if (chunk.length) chunks.push(chunk);
	return chunks;
}

export function lintTracked(cwd, run = spawnSync) {
	const files = trackedFiles(cwd);
	console.log(`Linting ${files.length} tracked frontend files (read-only).`);
	const tools = [
		[
			"biome",
			["check", "--files-ignore-unknown=true", "--no-errors-on-unmatched"],
		],
		[
			"oxlint",
			[
				"--import-plugin",
				"--tsconfig",
				"./tsconfig.json",
				"--ignore-path",
				".eslintignore",
			],
		],
	];
	const chunks = batches(files);
	for (const [tool, options] of tools) {
		for (const chunk of chunks) {
			const result = run(path.join(BIN, tool), [...options, ...chunk], {
				cwd,
				stdio: "inherit",
			});
			if (result.error) throw result.error;
			if (result.status !== 0) return result.status ?? 1;
		}
	}
	return 0;
}

if (
	process.argv[1] &&
	path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
	process.exit(lintTracked(process.cwd()));
