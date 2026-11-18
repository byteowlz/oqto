// Vendor a waqy release into Oqto's static assets, verifying every hash from the manifest.
//
// The model is data: Oqto loads the same release folder that the native `waqy` detector
// uses, straight from the front end's public directory. Nothing is guessed: the manifest
// lists a sha256 per file and this script refuses to copy a release that does not match.
//
//   WAQY_MODELS=~/.local/share/byteowlz/models/waqy WAQY_RELEASE=spqx-pub1 \
//     bun scripts/sync-waqy-runtime-assets.mjs
//
// Output: frontend/public/waqy/<release>/ (git-ignored) plus provenance.json.
import { createHash } from "node:crypto";
import {
	copyFileSync,
	existsSync,
	mkdirSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const frontend = fileURLToPath(new URL("../", import.meta.url));
export const DEFAULT_MODELS = path.join(
	process.env.HOME ?? "",
	".local/share/byteowlz/models/waqy",
);
// The published Hey Oqto release (huggingface.co/byteowlz/waqy, revision
// 76903376049f4f5fd8433104d4e46207e32127e0, manifest sha256 4b22f9d44c77...).
// Every consumer verifies the same per-file hashes from its manifest.
export const DEFAULT_RELEASE = "spqx-pub1";

const sha256 = (file) =>
	createHash("sha256").update(readFileSync(file)).digest("hex");

/** Verify a release folder and copy it into `target`. Returns the provenance record. */
export function syncRelease({ models, release, target, sourceCommit }) {
	const source = path.join(models, release);
	const manifestPath = path.join(source, "manifest.json");
	if (!existsSync(manifestPath)) {
		throw new Error(`no manifest at ${manifestPath}`);
	}
	const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
	const files = Object.entries(manifest.files ?? {});
	if (!files.length) throw new Error("manifest lists no files");
	if (
		!files.some(([key]) => key === "mel") ||
		!files.some(([key]) => key === "embedding")
	) {
		throw new Error("manifest must list the mel and embedding front end");
	}
	for (const [key, entry] of files) {
		if (path.basename(entry.file) !== entry.file) {
			throw new Error(`manifest file name for '${key}' must be a plain name`);
		}
		const file = path.join(source, entry.file);
		if (!existsSync(file))
			throw new Error(`missing ${entry.file} for '${key}'`);
		const digest = sha256(file);
		if (digest !== entry.sha256) {
			throw new Error(
				`checksum mismatch for ${entry.file}: manifest ${entry.sha256}, file ${digest}`,
			);
		}
	}
	if (existsSync(target)) rmSync(target, { recursive: true, force: true });
	mkdirSync(target, { recursive: true });
	for (const entry of files.map(([, e]) => e)) {
		copyFileSync(path.join(source, entry.file), path.join(target, entry.file));
	}
	const provenance = {
		release: manifest.release,
		threshold: manifest.threshold,
		classifiers: manifest.classifiers,
		files: Object.fromEntries(
			files.map(([key, entry]) => [
				key,
				{ file: entry.file, sha256: entry.sha256 },
			]),
		),
		bytes:
			files.reduce((sum, [, entry]) => sum + (entry.bytes ?? 0), 0) ||
			undefined,
		syncedAt: new Date().toISOString(),
		source: { models, release },
		sourceCommit: sourceCommit ?? null,
		note: "Vendored copy of an immutable waqy release. Hashes verified on sync; Oqto re-verifies at load.",
	};
	writeFileSync(
		path.join(target, "provenance.json"),
		`${JSON.stringify(provenance, null, 2)}\n`,
	);
	return provenance;
}

if (import.meta.main) {
	const models = process.env.WAQY_MODELS ?? DEFAULT_MODELS;
	const release = process.env.WAQY_RELEASE ?? DEFAULT_RELEASE;
	const target = path.join(frontend, "public/waqy", release);
	const provenance = syncRelease({
		models,
		release,
		target,
		sourceCommit: process.env.WAQY_SOURCE_COMMIT ?? null,
	});
	console.log(
		`vendored ${provenance.release} -> public/waqy/${release} (${provenance.classifiers.length} classifiers, ${Object.keys(provenance.files).length} files)`,
	);
}
