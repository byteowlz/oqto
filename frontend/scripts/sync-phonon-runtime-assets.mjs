import { createHash } from "node:crypto";
import { cpSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const frontend = fileURLToPath(new URL("../", import.meta.url));
const manifest = JSON.parse(
	readFileSync(
		new URL("./phonon-runtime-manifest.json", import.meta.url),
		"utf8",
	),
);
export function verifyPhononRuntime(source, lock) {
	const pkg = JSON.parse(
		readFileSync(path.join(source, "package.json"), "utf8"),
	);
	if (
		pkg.name !== "phonon-tts" ||
		pkg.version !== manifest.version ||
		!lock.includes(manifest.integrity)
	) {
		throw new Error(
			"Phonon requires the exact 0.4.0 package and pinned npm integrity",
		);
	}
	for (const [file, expected] of Object.entries(manifest.files)) {
		const hash = createHash("sha256")
			.update(readFileSync(path.join(source, file)))
			.digest("hex");
		if (hash !== expected)
			throw new Error(`Phonon runtime checksum mismatch: ${file}`);
	}
}
export function stagePhononRuntime(
	source = path.join(frontend, "node_modules/phonon-tts"),
	destination = path.join(frontend, "public/speech/phonon-0.4.0"),
) {
	verifyPhononRuntime(
		source,
		readFileSync(path.join(frontend, "bun.lock"), "utf8"),
	);
	mkdirSync(destination, { recursive: true });
	for (const file of Object.keys(manifest.files)) {
		mkdirSync(path.dirname(path.join(destination, file)), { recursive: true });
		cpSync(path.join(source, file), path.join(destination, file));
	}
	writeFileSync(
		path.join(destination, "runtime-manifest.json"),
		`${JSON.stringify(manifest, null, 2)}\n`,
	);
}
if (
	process.argv[1] &&
	path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
	stagePhononRuntime();
}
