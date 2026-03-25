import { promises as fs } from "node:fs";
import path from "node:path";

const ROOT = process.cwd();
const BASELINE_PATH = path.join(
	ROOT,
	"scripts",
	"useeffect-guardrail-baseline.json",
);
const TARGET_DIRS = ["src", "hooks", "features", "apps", "components", "lib"];
const FILE_EXTENSIONS = new Set([".ts", ".tsx"]);
const EFFECT_PATTERN = /(?:\bReact\.)?useEffect\s*\(/g;
const ALLOW_MARKER = "useeffect-guardrail: allow";

async function walk(dir) {
	const entries = await fs.readdir(dir, { withFileTypes: true });
	const files = [];
	for (const entry of entries) {
		const fullPath = path.join(dir, entry.name);
		if (entry.isDirectory()) {
			files.push(...(await walk(fullPath)));
			continue;
		}
		if (!FILE_EXTENSIONS.has(path.extname(entry.name))) {
			continue;
		}
		files.push(fullPath);
	}
	return files;
}

function shouldIgnoreMatch(lines, index) {
	const line = lines[index] ?? "";
	if (/^\s*import\b/.test(line)) {
		return true;
	}

	const start = Math.max(0, index - 2);
	for (let i = start; i <= index; i++) {
		if ((lines[i] ?? "").includes(ALLOW_MARKER)) {
			return true;
		}
	}
	return false;
}

async function collectUseEffectCounts() {
	const files = [];
	for (const target of TARGET_DIRS) {
		const abs = path.join(ROOT, target);
		files.push(...(await walk(abs)));
	}

	const counts = {};
	for (const file of files) {
		const relativePath = path.relative(ROOT, file).replaceAll(path.sep, "/");
		const text = await fs.readFile(file, "utf8");
		const lines = text.split(/\r?\n/);
		let count = 0;
		for (let i = 0; i < lines.length; i++) {
			const line = lines[i] ?? "";
			const matches = line.match(EFFECT_PATTERN);
			if (!matches || shouldIgnoreMatch(lines, i)) {
				continue;
			}
			count += matches.length;
		}

		if (count > 0) {
			counts[relativePath] = count;
		}
	}

	return counts;
}

function sortObject(input) {
	return Object.fromEntries(
		Object.entries(input).sort(([a], [b]) => a.localeCompare(b)),
	);
}

async function readBaseline() {
	const raw = await fs.readFile(BASELINE_PATH, "utf8");
	const parsed = JSON.parse(raw);
	if (typeof parsed !== "object" || parsed === null || !parsed.files) {
		throw new Error("Invalid useEffect guardrail baseline format");
	}
	return parsed;
}

async function writeBaseline(counts) {
	const payload = {
		generatedAt: new Date().toISOString(),
		allowMarker: ALLOW_MARKER,
		files: sortObject(counts),
	};
	await fs.writeFile(
		BASELINE_PATH,
		`${JSON.stringify(payload, null, 2)}\n`,
		"utf8",
	);
}

function compareCounts(current, baseline) {
	const violations = [];
	for (const [file, count] of Object.entries(current)) {
		const baselineCount = baseline[file] ?? 0;
		if (count > baselineCount) {
			violations.push({ file, count, baselineCount });
		}
	}
	return violations.sort((a, b) => a.file.localeCompare(b.file));
}

async function main() {
	const updateBaseline = process.argv.includes("--update-baseline");
	const counts = await collectUseEffectCounts();

	if (updateBaseline) {
		await writeBaseline(counts);
		const total = Object.values(counts).reduce((sum, value) => sum + value, 0);
		console.log(
			`Updated useEffect baseline (${Object.keys(counts).length} files, ${total} occurrences).`,
		);
		return;
	}

	const baseline = await readBaseline();
	const baselineFiles = baseline.files ?? {};
	const violations = compareCounts(counts, baselineFiles);

	if (violations.length > 0) {
		console.error(
			"useEffect guardrail violated. New unapproved useEffect usage detected:",
		);
		for (const violation of violations) {
			console.error(
				`  - ${violation.file}: ${violation.count} (baseline ${violation.baselineCount})`,
			);
		}
		console.error(
			`Add '${ALLOW_MARKER}: <reason>' above legitimate new useEffect blocks or reduce usage.`,
		);
		process.exit(1);
	}

	const total = Object.values(counts).reduce((sum, value) => sum + value, 0);
	console.log(
		`useEffect guardrail OK (${Object.keys(counts).length} files, ${total} tracked occurrences).`,
	);
}

main().catch((error) => {
	console.error(error instanceof Error ? error.message : String(error));
	process.exit(1);
});
