// Early headless CLI: no runtime preparation, profile discovery or SDK loader.
import { discoverClassicSources, readClassicSource } from "./classic-source.ts";

export async function runClassicInspection(args: string[]) {
	const [command, path, ...rest] = args;
	if (
		(command !== "inspect-classic" && command !== "list-classic") ||
		!path ||
		path.startsWith("--")
	)
		throw new Error(
			"Usage: oqt inspect-classic FILE --origin NAME [--leaf ID] --json | oqt list-classic DIR --origin NAME --json",
		);
	let origin: string | undefined;
	let leaf: string | undefined;
	const seen = new Set<string>();
	for (let index = 0; index < rest.length; index++) {
		const flag = rest[index];
		if (seen.has(flag)) throw new Error("Duplicate inspection option");
		seen.add(flag);
		if (flag === "--json") continue;
		if (flag !== "--origin" && flag !== "--leaf")
			throw new Error("Unsupported inspection option");
		const value = rest[++index];
		if (!value || value.startsWith("--"))
			throw new Error("Inspection option requires a value");
		if (flag === "--origin") origin = value;
		else leaf = value;
	}
	if (!origin) throw new Error("Explicit --origin NAME is required");
	if (command === "list-classic") {
		if (leaf !== undefined)
			throw new Error("--leaf is only valid for a selected source");
		return discoverClassicSources(path, origin);
	}
	const source = await readClassicSource(path, origin);
	if (leaf !== undefined && !Object.hasOwn(source.snapshot.entryDigests, leaf))
		throw new Error("Selected native leaf ID does not exist in source");
	return {
		sourceId: source.snapshot.sourceId,
		sourceHash: source.sourceHash,
		nativeSessionId: source.snapshot.header.id,
		sourceCwd: source.snapshot.header.cwd,
		sourceBytes: source.bytes.length,
		entries: source.snapshot.records.length,
		leaves: source.snapshot.leaves,
		selectedLeaf: leaf ?? source.snapshot.activeLeaf,
		validation: "archive_preflight_only",
		activation: "requires_worker_validation_and_explicit_import",
	};
}

if (process.argv[1]?.endsWith("classic-inspect.ts")) {
	try {
		console.log(
			JSON.stringify(await runClassicInspection(process.argv.slice(2))),
		);
	} catch (error) {
		console.error(
			JSON.stringify({
				error: error instanceof Error ? error.message : String(error),
			}),
		);
		process.exitCode = 2;
	}
}
