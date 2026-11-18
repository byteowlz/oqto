#!/usr/bin/env node
import { execFileSync } from "node:child_process";
// Stage the Oqtopus mascot's vector player into public/oqtopus/ (gitignored).
//
// Source: argv[2], $OQTOPUS_LIBRARY, or the sibling ../../oqto-animation/library.
// Only the live-player inputs are copied (library/transitions/bridges JSON,
// vector frames, player.js, the rest logo), never videos or raw takes. The
// player itself is used unchanged through its documented embedding contract;
// embed.html is Oqto's host page for it.
//
// A missing source is not an error: builds without the mascot simply keep the
// plain microphone icon. An existing staged copy is left in place.
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const frontendRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(
	process.argv[2] ??
		process.env.OQTOPUS_LIBRARY ??
		join(frontendRoot, "../../oqto-animation/library"),
);
const destination = join(frontendRoot, "public/oqtopus");

if (
	!existsSync(join(source, "library.json")) ||
	!existsSync(join(source, "player.js"))
) {
	// Only claim the icon fallback when nothing was staged by another machine.
	const staged = existsSync(join(destination, "library.json"));
	console.log(
		staged
			? `[oqtopus] No source library at ${source}; keeping the staged copy in public/oqtopus.`
			: `[oqtopus] No mascot library at ${source} and nothing staged; the plain microphone icon is used.`,
	);
	process.exit(0);
}

const library = JSON.parse(
	await readFile(join(source, "library.json"), "utf8"),
);
const files = new Set(["library.json", "player.js"]);
for (const optional of ["transitions.json", "bridges.json", "rest.svg"]) {
	if (existsSync(join(source, optional))) files.add(optional);
}
if (library.rest_vec) files.add(library.rest_vec);
for (const [name, clip] of Object.entries(library.clips ?? {})) {
	if (!clip.vec) throw new Error(`[oqtopus] clip '${name}' has no vector file`);
	files.add(`${name}/${clip.vec}`);
}
if (files.has("bridges.json")) {
	const bridges = JSON.parse(
		await readFile(join(source, "bridges.json"), "utf8"),
	);
	for (const bridge of bridges.bridges ?? [])
		if (bridge.vec) files.add(bridge.vec);
}
for (const file of files) {
	if (file.includes("..") || file.startsWith("/"))
		throw new Error(`[oqtopus] refusing path outside the library: ${file}`);
	if (!existsSync(join(source, file)))
		throw new Error(`[oqtopus] library lists a missing file: ${file}`);
}

await rm(destination, { recursive: true, force: true });
const hashes = {};
for (const file of [...files].sort()) {
	const out = join(destination, file);
	await mkdir(dirname(out), { recursive: true });
	await copyFile(join(source, file), out);
	hashes[file] = createHash("sha256")
		.update(await readFile(out))
		.digest("hex");
}

// Host page: the player expects its control elements; Oqto keeps them hidden and
// drives it through window.oqtopus instead.
await writeFile(
	join(destination, "embed.html"),
	`<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Oqtopus</title>
<style>
:root { color-scheme: dark; --oq-body: #ffffff; --oq-stripe: #cccccc; }
:root[data-theme="light"] { color-scheme: light; --oq-body: #333333; --oq-stripe: #1a1a1a; }
html, body { margin: 0; height: 100%; background: transparent; overflow: hidden; }
.stage { position: relative; width: 100%; height: 100%; }
.stage canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
</style>
</head>
<body>
<div class="stage"><canvas id="stage" aria-hidden="true"></canvas></div>
<div hidden>
<span id="status"></span><div id="actions"></div>
<input type="checkbox" id="idle"><input type="checkbox" id="cuts">
<select id="rest"><option value="none">none</option><option value="short">short</option><option value="normal">normal</option></select>
<select id="breath"><option value="off">off</option><option value="subtle">subtle</option><option value="soft">soft</option></select>
<select id="speed"><option value="1">1</option></select>
<select id="theme"><option value="auto">auto</option><option value="dark">dark</option><option value="light">light</option></select>
<select id="bg"><option value="theme">theme</option><option value="checker">checker</option></select>
</div>
<script type="module" src="player.js"></script>
</body>
</html>
`,
);

let revision = null;
try {
	revision = execFileSync("git", ["-C", source, "rev-parse", "HEAD"], {
		encoding: "utf8",
	}).trim();
} catch {
	revision = null;
}
await writeFile(
	join(destination, "PROVENANCE.json"),
	`${JSON.stringify({ source: "oqto-animation/library", revision, files: hashes }, null, 2)}\n`,
);
console.log(
	`[oqtopus] Staged ${files.size} mascot files from ${source}${revision ? ` @ ${revision.slice(0, 9)}` : ""}.`,
);
