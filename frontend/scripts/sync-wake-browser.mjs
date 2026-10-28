import { cpSync, existsSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Explicit maintainer action, not a build-time dependency on a sibling checkout.
const source = process.argv[2] ?? fileURLToPath(new URL("../../../waqy/", import.meta.url));
if (!existsSync(path.join(source, "web/waqy-engine.mjs")))
	throw new Error("Provide a waqy checkout with web/waqy-engine.mjs");
const destination = fileURLToPath(new URL("../vendor/waqy-browser/", import.meta.url));
mkdirSync(destination, { recursive: true });
for (const [from, to] of [
	["web/waqy-engine.mjs", "waqy-engine.mjs"],
	["LICENSE", "LICENSE"],
	["NOTICE", "NOTICE"],
]) {
	cpSync(path.join(source, from), path.join(destination, to));
}
console.log("Synced the waqy browser engine; update PROVENANCE.json with the source commit.");
