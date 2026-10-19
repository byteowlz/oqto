import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { lstat, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const launcher = fileURLToPath(new URL("../../oqt", import.meta.url));
test("headless inspection/discovery bypasses runtime preparation and absent owner profiles", async () => {
	const dir = await mkdtemp("/var/tmp/oqt-classic-cli-");
	try {
		const source = join(dir, "source.jsonl");
		const raw = `${JSON.stringify({ type: "session", version: 3, id: "9267f005-48a0-4c42-920e-3277aa2c6070", cwd: "/remote/mac", timestamp: "2026-10-17T00:00:00Z" })}\n`;
		await writeFile(source, raw, { mode: 0o400 });
		const env = {
			...process.env,
			HOME: join(dir, "absent-home"),
			OQT_OWNER_AGENT_DIR: join(dir, "absent-profile"),
			OQT_PI_SOURCE: join(dir, "absent-sdk"),
		};
		const inspect = JSON.parse(
			execFileSync(
				"bash",
				[launcher, "inspect-classic", source, "--origin", "macbook", "--json"],
				{ env, encoding: "utf8", timeout: 10000 },
			),
		);
		assert.equal(
			inspect.sourceId,
			"pi:macbook:9267f005-48a0-4c42-920e-3277aa2c6070",
		);
		assert.equal(inspect.validation, "archive_preflight_only");
		assert.equal(inspect.sourceCwd, "/remote/mac");
		const list = JSON.parse(
			execFileSync(
				"bash",
				[launcher, "list-classic", dir, "--origin", "macbook", "--json"],
				{ env, encoding: "utf8", timeout: 10000 },
			),
		);
		assert.equal(list.sources.length, 1);
		assert.equal(await readFile(source, "utf8"), raw);
		await assert.rejects(lstat(env.HOME), /ENOENT/);
		await assert.rejects(lstat(env.OQT_OWNER_AGENT_DIR), /ENOENT/);
	} finally {
		await rm(dir, { recursive: true, force: true });
	}
});
