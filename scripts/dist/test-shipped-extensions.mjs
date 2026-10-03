#!/usr/bin/env node
/** Real Pi 1.0 factory/session-start/command smoke for every local extension.
 * Uses disposable HOME, stripped credentials and stubbed service CLIs.
 * This does not prove every tool callback or physical TUI interaction.
 */
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { StringDecoder } from "node:string_decoder";

const binary = process.argv[2] ? resolve(process.argv[2]) : undefined;
assert(binary && process.argv[3], "Pass exact Pi1 binary and staged extension directory");
assert.equal(execFileSync(binary, ["--version"], { encoding: "utf8", timeout: 10000 }).trim(), "1.0.0");
const root = resolve(process.argv[3]);
const extensions = (await readdir(root, { withFileTypes: true }))
	.filter((entry) => entry.isDirectory() && entry.name.startsWith("pi-"))
	.map((entry) => join(root, entry.name, "index.ts"));
assert(extensions.length > 0, "No extensions discovered");
const temp = await mkdtemp(join(tmpdir(), "pi-1-catalog-"));
const home = join(temp, "home");
const project = join(temp, "project");
const cli = join(temp, "bin");
await mkdir(join(home, ".pi", "agent"), { recursive: true });
await mkdir(project);
await mkdir(cli);
// Factory/startup must not reach real vaults, shared stores or orchestration.
for (const name of ["kyz", "mmry", "agntz", "herdr", "trx", "xlatch", "eavs", "hstry"]) {
	await writeFile(join(cli, name), "#!/bin/sh\nexit 127\n", { mode: 0o755 });
}
await writeFile(
	join(home, ".pi", "agent", "models.json"),
	JSON.stringify({
		providers: {
			"catalog-fixture": {
				baseUrl: "http://127.0.0.1:9/v1",
				api: "openai-completions",
				apiKey: "synthetic",
				models: [{ id: "fixture", input: ["text"], contextWindow: 4096, maxTokens: 256 }],
			},
		},
	})
);
const probe = join(temp, "probe.ts");
await writeFile(
	probe,
	`export default function(pi) {
  pi.registerCommand("pi_1_catalog_probe", { description: "Synthetic catalog probe", handler: (_args, ctx) => {
    ctx.ui.setStatus("pi_1_catalog_probe", JSON.stringify({mode: ctx.mode, tools: pi.getAllTools().map(t => t.name)}));
  }});
}`
);
const cases = extensions.map((extension) => ({ name: dirname(extension).split("/").at(-1), paths: [extension] }));
cases.push({ name: "all-local-extensions", paths: extensions });
const failures = [];
try {
	for (const entry of cases) {
		const result = await new Promise((resolveResult) => {
			const child = spawn(
				binary,
				[
					"--mode",
					"rpc",
					"--offline",
					"-ne",
					"-ns",
					"-np",
					"-nc",
					"-na",
					"--no-tools",
					"--no-session",
					"--provider",
					"catalog-fixture",
					"--model",
					"fixture",
					...entry.paths.flatMap((extension) => ["-e", extension]),
					"-e",
					probe,
				],
				{
					cwd: project,
					env: {
						PATH: `${cli}:/usr/bin:/bin`,
						HOME: home,
						PI_CODING_AGENT_DIR: join(home, ".pi", "agent"),
						PI_OFFLINE: "1",
						XDG_CONFIG_HOME: join(home, ".config"),
						XDG_CACHE_HOME: join(home, ".cache"),
						XDG_DATA_HOME: join(home, ".local", "share"),
						XDG_RUNTIME_DIR: temp,
					},
					stdio: ["pipe", "pipe", "pipe"],
				}
			);
			const rows = [];
			const decoder = new StringDecoder("utf8");
			let buffer = "";
			let stderr = "";
			let response;
			let probeResult;
			let deadline = false;
			const timer = setTimeout(() => {
				deadline = true;
				child.kill("SIGKILL");
			}, 20000);
			child.stderr.on("data", (chunk) => {
				stderr += chunk.toString();
			});
			child.stdout.on("data", (chunk) => {
				buffer += decoder.write(chunk);
				while (buffer.includes("\n")) {
					const boundary = buffer.indexOf("\n");
					const line = buffer.slice(0, boundary);
					buffer = buffer.slice(boundary + 1);
					try {
						const row = JSON.parse(line);
						rows.push(row);
						if (row.type === "response" && row.id === "probe") {
							response = row;
							child.stdin.end();
						}
						if (row.statusKey === "pi_1_catalog_probe") probeResult = JSON.parse(row.statusText);
					} catch (error) {
						rows.push({ type: "invalid_protocol", error: String(error) });
						child.kill("SIGKILL");
					}
				}
			});
			child.on("error", (error) => {
				stderr += String(error);
			});
			child.on("close", (code) => {
				clearTimeout(timer);
				if ((buffer + decoder.end()).trim()) rows.push({ type: "invalid_protocol", error: "Unterminated RPC frame" });
				resolveResult({ code, deadline, stderr, response, probeResult, rows });
			});
			child.stdin.on("error", () => undefined);
			child.stdin.write(`${JSON.stringify({ id: "probe", type: "prompt", message: "/pi_1_catalog_probe" })}\n`);
		});
		const name = entry.name;
		const errors = result.rows.filter((row) => ["extension_error", "invalid_protocol"].includes(row.type));
		const ok =
			result.code === 0 &&
			!result.deadline &&
			result.response?.success === true &&
			result.response?.data?.disposition === "handled" &&
			result.probeResult?.mode === "rpc" &&
			errors.length === 0 &&
			!/Failed to load|Error loading|SyntaxError|TypeError/.test(result.stderr);
		console.log(`${ok ? "PASS" : "FAIL"} ${name}`);
		if (!ok) failures.push({ name, ...result });
	}
	if (failures.length) {
		console.error(JSON.stringify(failures, null, 2));
		process.exitCode = 1;
	} else {
		console.log(`${extensions.length} extensions and the combined catalog passed real Pi 1.0 catalog/session-start smoke.`);
	}
} finally {
	await rm(temp, { recursive: true, force: true });
}
