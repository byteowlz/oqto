import { cpSync, existsSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Explicit maintainer action, not a build-time dependency on a sibling checkout.
const source =
	process.argv[2] ??
	fileURLToPath(new URL("../../../eaRS/packages/browser/", import.meta.url));
const destination = fileURLToPath(
	new URL("../vendor/ears-browser/", import.meta.url),
);
if (!existsSync(path.join(source, "package.json")))
	throw new Error("Provide an eaRS packages/browser source directory");
mkdirSync(destination, { recursive: true });
for (const entry of [
	"package.json",
	"src",
	"scripts",
	"README.md",
	"LICENSE",
]) {
	cpSync(path.join(source, entry), path.join(destination, entry), {
		recursive: true,
	});
}
console.log(
	"Synced the standalone eaRS browser SDK source; run bun install and sync:speech-runtime-assets.",
);
