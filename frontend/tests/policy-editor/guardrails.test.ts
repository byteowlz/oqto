import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
function check(files: Record<string, string>) {
	const root = mkdtempSync(path.join(tmpdir(), "policy-boundary-"));
	try {
		for (const [name, text] of Object.entries(files)) {
			const dest = path.join(root, name);
			mkdirSync(path.dirname(dest), { recursive: true });
			writeFileSync(dest, text);
		}
		return spawnSync(
			process.execPath,
			[
				path.resolve("scripts/check-oqto-ui-guardrails.mjs"),
				"--source-root",
				root,
			],
			{ encoding: "utf8" },
		);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
}
describe("policy-editor architecture boundary", () => {
	it("accepts own layer, platform, and synthetic dev presentation", () => {
		const result = check({
			"platform/policy.ts": "export const policy = 1;",
			"policy-editor/model.ts":
				'import { policy } from "../platform/policy"; export const model = policy;',
			"policy-editor/view.ts":
				'import { model } from "./model"; export const view = model;',
			"dev/policy-editor/preview.ts":
				'import { view } from "../../policy-editor/view"; export const preview = view;',
		});
		expect(result.status, result.stderr).toBe(0);
	});
	it.each(["chat", "files", "dev"])(
		"rejects feature imports from %s",
		(layer) => {
			const result = check({
				[`${layer}/model.ts`]: "export const other = 1;",
				"policy-editor/model.ts": `import { other } from "../${layer}/model"; export const model = other;`,
			});
			expect(result.status).toBe(1);
			expect(result.stderr).toContain("architecture/layer-direction");
		},
	);
	it("rejects legacy aliases", () => {
		const result = check({
			"policy-editor/model.ts":
				'import { value } from "@/lib/api/policy"; export const model = value;',
		});
		expect(result.status).toBe(1);
		expect(result.stderr).toContain("architecture/");
	});
	it("rejects platform reaching into policy editor", () => {
		const result = check({
			"policy-editor/model.ts": "export const model = 1;",
			"platform/policy.ts":
				'import { model } from "../policy-editor/model"; export const policy = model;',
		});
		expect(result.status).toBe(1);
		expect(result.stderr).toContain("architecture/layer-direction");
	});
});
