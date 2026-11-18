import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const roots: string[] = [];
afterEach(() => {
	for (const root of roots.splice(0))
		rmSync(root, { recursive: true, force: true });
});

function checkCommand(source: string) {
	const scratch = path.resolve(".tmp");
	mkdirSync(scratch, { recursive: true });
	const root = mkdtempSync(path.join(scratch, "command-types-"));
	roots.push(root);
	writeFileSync(
		path.join(root, "case.ts"),
		`
		import type { getWsManager } from "@/lib/ws-manager";
		type Command = Parameters<ReturnType<typeof getWsManager>["sendAndWait"]>[0];
		${source}
	`,
	);
	writeFileSync(
		path.join(root, "tsconfig.json"),
		JSON.stringify({
			extends: path.resolve("tsconfig.oqto-ui.json"),
			compilerOptions: { incremental: false },
			include: ["case.ts"],
		}),
	);
	return spawnSync(
		"bun",
		[
			path.resolve("node_modules/typescript/bin/tsc"),
			"--project",
			path.join(root, "tsconfig.json"),
			"--noEmit",
		],
		{
			encoding: "utf8",
			timeout: 30_000,
		},
	);
}

describe("WebSocket command variants before request ID assignment", () => {
	it("retains each real wire variant's fields without changing the command", () => {
		const result = checkCommand(`
			export const agent: Command = { channel: "agent", session_id: "synthetic-session", cmd: "get_state" };
			export const files: Command = { channel: "files", type: "tree", path: ".", workspace_path: "/synthetic" };
		`);
		expect(result.error).toBeUndefined();
		expect(result.stdout + result.stderr).toBe("");
		expect(result.status).toBe(0);
	}, 30_000);

	it("rejects mixed channels, missing routing fields and caller-assigned request IDs", () => {
		const result = checkCommand(`
			export const mixed: Command = { channel: "files", session_id: "synthetic-session", cmd: "get_state" };
			export const missing: Command = { channel: "agent", cmd: "get_state" };
			export const assigned: Command = { channel: "agent", session_id: "synthetic-session", cmd: "get_state", id: "not-caller-owned" };
		`);
		expect(result.error).toBeUndefined();
		expect(result.status).not.toBe(0);
		expect(result.stdout).toContain("case.ts");
		expect(result.stdout).toMatch(/TS(2322|2353)/);
		expect(result.stdout.match(/error TS/g)).toHaveLength(3);
	}, 30_000);
});
