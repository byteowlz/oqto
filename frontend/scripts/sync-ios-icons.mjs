// Maintenance only: public builds consume the generated data, never a sibling repo.
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const [source, commit] = process.argv.slice(2);
if (!source || !/^[a-f0-9]{7,40}$/.test(commit ?? "")) {
	throw new Error(
		"Usage: bun scripts/sync-ios-icons.mjs <oqto-mobile-source> <source-commit>",
	);
}
const app = path.join(source, "apps/ios/OqtoMobile");
const iconSource = await readFile(
	path.join(app, "Theme/OqtoIcon.swift"),
	"utf8",
);
const declaration = iconSource.split("enum OqtoIcon:")[1]?.split("var id:")[0];
if (!declaration) throw new Error("No native semantic icon declaration");
const names = [...declaration.matchAll(/\bcase ([^\n]+)/g)].flatMap((match) =>
	match[1].split(",").map((name) => name.trim()),
);
if (
	names.length < 40 ||
	new Set(names).size !== names.length ||
	names.some((name) => !/^[a-zA-Z]+$/.test(name))
) {
	throw new Error("Unexpected native semantic icon inventory");
}
const glyphs = {};
const originals = {};
for (const name of names.sort()) {
	const relative = `Resources/Icons.xcassets/Icons/${name}.imageset/${name}.svg`;
	const bytes = await readFile(path.join(app, relative));
	const svg = bytes.toString("utf8");
	const viewBox = svg.match(/\bviewBox="([^"]+)"/)?.[1];
	if (
		!viewBox ||
		!/^-?[\d.]+(?: -?[\d.]+){3}$/.test(viewBox) ||
		/<(?!\/?(?:svg|path)\b)[^>]+>/.test(svg)
	) {
		throw new Error(`Unexpected vector structure: ${name}`);
	}
	const paths = [...svg.matchAll(/\bd="([^"]+)"/g)].map((match) => match[1]);
	if (paths.length === 0) throw new Error(`Missing vector path: ${name}`);
	glyphs[name] = { viewBox, paths };
	originals[name] = {
		path: relative,
		sha256: createHash("sha256").update(bytes).digest("hex"),
	};
}
const destination = path.resolve("src/oqto-ui/platform/icons");
await mkdir(destination, { recursive: true });
await writeFile(
	path.join(destination, "sharp-paths.ts"),
	`// Generated from Oqto iOS Material Symbols sharp400 assets; Apache-2.0.\n// Run scripts/sync-ios-icons.mjs with an explicit reviewed source. Do not edit.\nexport const sharpPaths = ${JSON.stringify(glyphs, null, "\t")} as const;\n`,
);
await writeFile(
	path.join(destination, "provenance.json"),
	`${JSON.stringify(
		{
			source_repo: "oqto-mobile",
			source_commit: commit,
			family: "Material Symbols sharp",
			weight: 400,
			license: "Apache-2.0",
			icons: originals,
		},
		null,
		"\t",
	)}\n`,
);
await writeFile(
	path.join(destination, "LICENSE"),
	await readFile(
		path.join(app, "Resources/Licenses/material-symbols-Apache-2.0.txt"),
	),
);
console.log(
	`Generated ${names.length} exact native semantic vectors; no runtime sibling dependency`,
);
