import { execFileSync } from "node:child_process";
import {
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

const roots: string[] = [];
const script = pathToFileURL(path.resolve("scripts/lint-tracked.mjs")).href;
afterEach(() => {
	for (const root of roots.splice(0))
		rmSync(root, { recursive: true, force: true });
});
function fixture() {
	const root = mkdtempSync(path.join(tmpdir(), "tracked-lint-"));
	roots.push(root);
	execFileSync("git", ["init", "--quiet", root]);
	return root;
}
function file(
	root: string,
	name: string,
	content = "export const value = 1;\n",
) {
	mkdirSync(path.dirname(path.join(root, name)), { recursive: true });
	writeFileSync(path.join(root, name), content);
}
function invoke(root: string, body: string) {
	const output = execFileSync(
		"bun",
		[
			"--eval",
			`
		import { lintTracked, trackedFiles } from ${JSON.stringify(script)};
		${body}
	`,
		],
		{ cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
	);
	return JSON.parse(output.trim().split("\n").at(-1) ?? "");
}

describe("tracked-source lint inventory", () => {
	it("includes every tracked path, preserves unusual names, excludes only untracked content, and never writes", () => {
		const root = fixture();
		file(root, ".gitignore", ".tmp/\npublic/oqtopus/\n");
		const names = [
			"source with spaces.ts",
			"line\nbreak.ts",
			"vendor/owned.ts",
			"public/oqtopus/tracked.ts",
		];
		for (const name of names) file(root, name);
		execFileSync("git", ["add", "--force", "--", ".gitignore", ...names], {
			cwd: root,
		});
		file(root, ".tmp/broken.ts", "this is not valid source");
		file(
			root,
			"public/oqtopus/huge-generated.json",
			"[".repeat(2 * 1024 * 1024),
		);
		file(root, "untracked.ts", "not source for the index yet");
		const before = names.map((name) =>
			readFileSync(path.join(root, name), "utf8"),
		);
		const result = invoke(
			root,
			`
			const calls = [];
			const status = lintTracked(process.cwd(), (command, args) => {
				calls.push({ command, args }); return { status: 0 };
			});
			console.log(JSON.stringify({ files: trackedFiles(process.cwd()), calls, status }));
		`,
		);
		expect(result.status).toBe(0);
		expect(result.files.sort()).toEqual([".gitignore", ...names].sort());
		for (const call of result.calls) {
			for (const name of names) expect(call.args).toContain(`./${name}`);
			expect(call.args).not.toContain(".");
			expect(call.args.some((arg: string) => /--(write|fix)/.test(arg))).toBe(
				false,
			);
		}
		expect(
			names.map((name) => readFileSync(path.join(root, name), "utf8")),
		).toEqual(before);
	});

	it("keeps generated tracked source read-only and reports real formatter failures", () => {
		const root = fixture();
		const name = "src/generated/unformatted.ts";
		file(root, name, "export    const x=1\n");
		execFileSync("git", ["add", "--", name], { cwd: root });
		const before = readFileSync(path.join(root, name), "utf8");
		const result = invoke(
			root,
			`
			import { spawnSync } from "node:child_process";
			const status = lintTracked(process.cwd(), (command, args, options) =>
				spawnSync(command, args, { ...options, stdio: "pipe" }));
			console.log(JSON.stringify({ status }));
		`,
		);
		expect(result.status).not.toBe(0);
		expect(readFileSync(path.join(root, name), "utf8")).toBe(before);
	});

	it("propagates tool failure and refuses missing tracked files instead of dropping source", () => {
		const root = fixture();
		file(root, "source.ts");
		execFileSync("git", ["add", "--", "source.ts"], { cwd: root });
		expect(
			invoke(
				root,
				"console.log(JSON.stringify({ status: lintTracked(process.cwd(), () => ({ status: 7 })) }));",
			).status,
		).toBe(7);
		rmSync(path.join(root, "source.ts"));
		expect(() => invoke(root, "trackedFiles(process.cwd());")).toThrow();
	});

	it("batches bounded argument lists without losing or filtering tracked files", () => {
		const root = fixture();
		const names = Array.from(
			{ length: 700 },
			(_, index) => `source/${index}-${"x".repeat(80)}.ts`,
		);
		for (const name of names) file(root, name);
		execFileSync("git", ["add", "--", "."], { cwd: root });
		const result = invoke(
			root,
			`
			const calls = [];
			lintTracked(process.cwd(), (command, args) => {
				calls.push({ command, files: args.filter(arg => arg.startsWith("./source/")) });
				return { status: 0 };
			});
			console.log(JSON.stringify(calls));
		`,
		);
		for (const tool of ["biome", "oxlint"]) {
			const calls = result.filter(
				(call: { command: string }) => path.basename(call.command) === tool,
			);
			expect(calls.length).toBeGreaterThan(1);
			const selected = calls.flatMap((call: { files: string[] }) => call.files);
			expect(selected.sort()).toEqual(names.map((name) => `./${name}`).sort());
			for (const call of calls)
				expect(
					Buffer.byteLength(call.files.join("\0")) + 1,
				).toBeLessThanOrEqual(32 * 1024);
		}
	});
});
