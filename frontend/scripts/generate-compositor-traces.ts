/**
 * Regenerates the compositor conformance-trace corpus from the reference
 * TypeScript kernel. Deterministic: running twice produces identical files.
 * The corpus directory is fully owned by this generator; stale files are
 * removed. Freshness is enforced by
 * frontend/tests/compositor/oqto-ui-compositor-traces.test.ts.
 *
 * Usage: bun scripts/generate-compositor-traces.ts
 */

import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { buildCompositorTraceCorpus } from "./compositor-traces/build";

const outputDir = path.resolve(
	import.meta.dir,
	"../../contracts/oqto-ui/traces/compositor",
);

const corpus = buildCompositorTraceCorpus();
mkdirSync(outputDir, { recursive: true });

const expected = new Set(corpus.map((file) => file.path));
for (const existing of readdirSync(outputDir)) {
	if (!expected.has(existing)) {
		rmSync(path.join(outputDir, existing));
		console.log(`removed stale ${existing}`);
	}
}

for (const file of corpus) {
	const target = path.join(outputDir, file.path);
	writeFileSync(target, `${JSON.stringify(file.data, null, "\t")}\n`, "utf8");
	console.log(`wrote ${path.relative(process.cwd(), target)}`);
}

console.log(`compositor trace corpus: ${corpus.length} files`);
