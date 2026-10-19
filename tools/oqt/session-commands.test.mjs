import assert from "node:assert/strict";
import { copyFile, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

const source = process.env.OQT_PI_SOURCE;
if (!source)
	throw new Error("OQT_PI_SOURCE must name pinned test dependencies");

test("classic resume parser and native prefix selection preserve explicit identity", async () => {
	const dir = await mkdtemp(join(tmpdir(), "oqt-session-commands-"));
	try {
		await symlink(join(source, "node_modules"), join(dir, "node_modules"));
		await symlink(
			join(source, "packages/coding-agent/src/experimental/services"),
			join(dir, "services"),
		);
		for (const name of ["session-commands.ts", "session-forks.ts"])
			await copyFile(new URL(name, import.meta.url), join(dir, name));
		const { createOqtSessionFacet } = await import(
			pathToFileURL(join(dir, "session-commands.ts"))
		);
		const { parseClassicResumeArgs: parse } = await import(
			"./classic-client.ts"
		);
		assert.deepEqual(
			parse('--classic "/archive/a b.jsonl" --origin macbook --leaf e1'),
			{ path: "/archive/a b.jsonl", origin: "macbook", leafId: "e1" },
		);
		assert.deepEqual(parse("--classic --origin macbook"), {
			origin: "macbook",
		});
		assert.deepEqual(parse("--classic /archive/a\\ b.jsonl --origin macbook"), {
			path: "/archive/a b.jsonl",
			origin: "macbook",
		});
		for (const bad of [
			"--classic /tmp/a",
			"--classic --origin ../bad",
			"--classic --origin",
			"--classic --origin a --origin b",
			"--classic --origin a --leaf x --leaf y",
			"--classic --classic --origin a",
			'--classic "unterminated --origin a',
			"--classic --origin a --unknown",
			"--origin a",
			"--classic a b --origin c",
			"--classic --origin a\\",
		])
			assert.throws(() => parse(bad), undefined, bad);
		const commands = new Map(),
			switched = [];
		const statuses = [];
		const ui = {
			showStatus(text) {
				statuses.push(text);
			},
			async select() {
				return undefined;
			},
		};
		const directory = {
			state: {
				value: {
					sessions: [
						{ sessionId: "abc-1", createdAt: 1 },
						{ sessionId: "abc-2", createdAt: 2 },
					],
				},
			},
		};
		const services = {
			"pi.local.slash-commands": {
				replace(command) {
					commands.set(command.name, command);
					return () => {};
				},
			},
			"pi.local.presentation-ui": ui,
			"pi.session-management": {},
			"pi.session-directory": directory,
			"oqt.session-forks": {},
		};
		const facet = createOqtSessionFacet({
			currentSessionId: () => "abc-1",
			async switchTo(id) {
				switched.push(id);
			},
		});
		facet.setup({
			use(service) {
				assert.ok(service.id in services, service.id);
				return services[service.id];
			},
			onActivate(callback) {
				callback();
			},
			own() {},
		});
		const resume = commands.get("resume");
		await assert.rejects(resume.run("abc", {}), /Ambiguous/);
		await assert.rejects(resume.run("missing", {}), /No session/);
		await resume.run("abc-2", {});
		assert.deepEqual(switched, ["abc-2"]);
		await resume.run("", {});
		assert.deepEqual(switched, ["abc-2"]);
		await writeFile(
			join(dir, "classic-client.ts"),
			`export {parseClassicResumeArgs} from ${JSON.stringify(new URL("./classic-client.ts", import.meta.url).href)}; export const calls = []; export async function importClassicSource(...args) { calls.push(args); return {sessionId: "imported-session"}; }`,
		);
		await writeFile(
			join(dir, "classic-source.ts"),
			`export const calls = []; export const result = {sources: [{path: "/fixture/archive.jsonl", sourceId: "pi:macbook:stable", cwd: "/must-not-adopt\\u001b[31m", sourceHash: "fixture-hash", leaves: ["e1"]}], skipped: [{path: "/fixture/skipped", reason: "fixture"}], truncated: true}; export async function discoverClassicSources(...args) { calls.push(args); return result; }`,
		);
		const oldOperator = process.env.OQT_GUARD_OPERATOR;
		const oldOwner = process.env.OQT_OWNER_AGENT_DIR;
		try {
			delete process.env.OQT_GUARD_OPERATOR;
			await assert.rejects(
				resume.run("--classic a --origin macbook", {}),
				/operator endpoint/,
			);
			process.env.OQT_GUARD_OPERATOR = "/fixture/operator.sock";
			const abort = new AbortController();
			await resume.run(
				'--classic "/fixture/a b.jsonl" --origin macbook --leaf e1',
				{ abortSignal: abort.signal },
			);
			const client = await import(
				pathToFileURL(join(dir, "classic-client.ts"))
			);
			assert.deepEqual(client.calls[0], [
				"/fixture/operator.sock",
				"/fixture/a b.jsonl",
				"macbook",
				{ leafId: "e1", signal: abort.signal },
			]);
			process.env.OQT_OWNER_AGENT_DIR = "/fixture/owner";
			await resume.run("--classic --origin macbook", {}); // picker cancellation
			assert.equal(client.calls.length, 1);
			assert.ok(
				statuses.includes("Classic discovery: 1 skipped; results truncated"),
			);
			const discovery = await import(
				pathToFileURL(join(dir, "classic-source.ts"))
			);
			discovery.result.skipped = [];
			discovery.result.truncated = false;
			statuses.length = 0;
			await resume.run("--classic --origin macbook", {});
			assert.deepEqual(
				statuses,
				[],
				"Empty skipped array must not emit a warning",
			);
			ui.select = async (_title, choices) => {
				assert.equal(choices[0].description.includes("\u001b"), false);
				assert.ok(choices[0].description.includes("\\u001b"));
				return "/fixture/archive.jsonl";
			};
			const beforeCwd = process.cwd();
			await resume.run("--classic --origin macbook", {});
			assert.deepEqual(discovery.calls[0], [
				"/fixture/owner/sessions",
				"macbook",
			]);
			assert.equal(client.calls[1][1], "/fixture/archive.jsonl");
			assert.equal(client.calls[1][3].expectedHash, "fixture-hash");
			assert.equal(switched.at(-1), "imported-session");
			assert.equal(process.cwd(), beforeCwd);
		} finally {
			if (oldOperator === undefined) delete process.env.OQT_GUARD_OPERATOR;
			else process.env.OQT_GUARD_OPERATOR = oldOperator;
			if (oldOwner === undefined) delete process.env.OQT_OWNER_AGENT_DIR;
			else process.env.OQT_OWNER_AGENT_DIR = oldOwner;
		}
	} finally {
		await rm(dir, { recursive: true, force: true });
	}
});
