import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const SCRIPT = path.resolve("scripts/check-workbench-guardrails.mjs");
const tempRoots: string[] = [];

function fixture(files: Record<string, string>): string {
	const root = mkdtempSync(path.join(tmpdir(), "oqto-workbench-guardrails-"));
	tempRoots.push(root);
	for (const [relative, content] of Object.entries(files)) {
		const target = path.join(root, relative);
		mkdirSync(path.dirname(target), { recursive: true });
		writeFileSync(target, content, "utf8");
	}
	return root;
}

function runFailure(files: Record<string, string>): string {
	const root = fixture(files);
	const result = spawnSync(process.execPath, [SCRIPT, "--source-root", root], {
		encoding: "utf8",
	});
	expect(result.status).toBe(1);
	return `${result.stdout}\n${result.stderr}`;
}

afterEach(() => {
	for (const root of tempRoots.splice(0)) {
		rmSync(root, { recursive: true, force: true });
	}
});

describe("Workbench architecture guardrails", () => {
	it("accepts an empty zero baseline", () => {
		const root = fixture({});
		const output = execFileSync(
			process.execPath,
			[SCRIPT, "--source-root", root],
			{ encoding: "utf8" },
		);
		expect(output).toContain("zero-baseline");
	});

	it("accepts the valid layer direction and adapter-owned browser APIs", () => {
		const root = fixture({
			"routes/main/route.ts":
				'import { shell } from "../../surfaces/shell/view";\nexport const route = () => shell();\n',
			"surfaces/shell/view.ts":
				'import { tabs } from "../../modules/tabs/model";\nexport const shell = () => tabs();\n',
			"modules/tabs/model.ts":
				'import { readLayout } from "../../adapters/layout/storage";\nexport const tabs = () => readLayout();\n',
			"adapters/layout/storage.ts":
				'export const readLayout = () => localStorage.getItem("oqto:layout");\n',
		});
		expect(() =>
			execFileSync(process.execPath, [SCRIPT, "--source-root", root]),
		).not.toThrow();
	});

	it("accepts react-i18next Trans fallback content", () => {
		const root = fixture({
			"surfaces/chat/View.tsx":
				'import { Trans } from "react-i18next";\nexport const View = () => <Trans i18nKey="chat.start">Start a <strong>new session</strong></Trans>;\n',
		});
		expect(() =>
			execFileSync(process.execPath, [SCRIPT, "--source-root", root]),
		).not.toThrow();
	});

	it("rejects an exception not approved by Tommy", () => {
		const root = fixture({
			"surfaces/chat/View.tsx":
				"export const View = () => <p>Unapproved text</p>;\n",
			"exceptions.json": JSON.stringify({
				version: 1,
				exceptions: [
					{
						rule: "i18n/untranslated-text",
						file: "surfaces/chat/View.tsx",
						reason: "An agent attempted to approve this",
						removalCondition: "Remove when localization is added",
						issue: "oqto-2jjm.1",
						approvedBy: "Agent",
					},
				],
			}),
		});
		const result = spawnSync(
			process.execPath,
			[
				SCRIPT,
				"--source-root",
				root,
				"--exceptions",
				path.join(root, "exceptions.json"),
			],
			{ encoding: "utf8" },
		);
		expect(result.status).toBe(1);
		expect(result.stderr).toContain("Tommy approval");
	});

	it("honors only an exact Tommy-approved exception manifest entry", () => {
		const root = fixture({
			"surfaces/chat/View.tsx":
				"export const View = () => <p>Temporary approved text</p>;\n",
			"exceptions.json": JSON.stringify({
				version: 1,
				exceptions: [
					{
						rule: "i18n/untranslated-text",
						file: "surfaces/chat/View.tsx",
						line: 1,
						reason: "Tommy approved this fixture exception",
						removalCondition: "Remove when the fixture test completes",
						issue: "oqto-2jjm.1",
						approvedBy: "Tommy",
					},
				],
			}),
		});
		expect(() =>
			execFileSync(
				process.execPath,
				[
					SCRIPT,
					"--source-root",
					root,
					"--exceptions",
					path.join(root, "exceptions.json"),
				],
				{ encoding: "utf8" },
			),
		).not.toThrow();
	});

	it.each([
		{
			name: "unknown source layer",
			rule: "architecture/unknown-layer",
			files: { "misc/value.ts": "export const value = 1;\n" },
		},
		{
			name: "legacy shell import",
			rule: "architecture/legacy-import",
			files: {
				"surfaces/chat/view.ts":
					'import { useApp } from "@/hooks/use-app";\nexport const view = useApp;\n',
			},
		},
		{
			name: "relative import escaping the Workbench boundary",
			rule: "architecture/source-boundary-import",
			files: {
				"adapters/chat/client.ts":
					'import { useApp } from "../../../hooks/use-app";\nexport const client = useApp;\n',
			},
		},
		{
			name: "legacy feature barrel import",
			rule: "architecture/legacy-import",
			files: {
				"surfaces/chat/view.ts":
					'import { ChatView } from "@/features/chat";\nexport const view = ChatView;\n',
			},
		},
		{
			name: "unapproved project alias",
			rule: "architecture/project-alias-import",
			files: {
				"adapters/apps/registry.ts":
					'import { appRegistry } from "@/lib/app-registry";\nexport const registry = appRegistry;\n',
			},
		},
		{
			name: "reverse layer dependency",
			rule: "architecture/layer-direction",
			files: {
				"modules/chat/model.ts":
					'import { view } from "../../surfaces/chat/view";\nexport const model = view;\n',
				"surfaces/chat/view.ts": "export const view = 1;\n",
			},
		},
		{
			name: "sideways module dependency",
			rule: "architecture/sideways-import",
			files: {
				"modules/chat/model.ts":
					'import { files } from "../files/model";\nexport const chat = files;\n',
				"modules/files/model.ts": "export const files = 1;\n",
			},
		},
		{
			name: "import cycle",
			rule: "architecture/import-cycle",
			files: {
				"modules/chat/a.ts": 'import { b } from "./b";\nexport const a = b;\n',
				"modules/chat/b.ts": 'import { a } from "./a";\nexport const b = a;\n',
			},
		},
		{
			name: "broad export-star barrel",
			rule: "architecture/broad-barrel",
			files: {
				"modules/chat/index.ts": 'export * from "./model";\n',
				"modules/chat/model.ts": "export const value = 1;\n",
			},
		},
		{
			name: "namespace export barrel",
			rule: "architecture/broad-barrel",
			files: {
				"modules/chat/index.ts": 'export * as model from "./model";\n',
				"modules/chat/model.ts": "export const value = 1;\n",
			},
		},
		{
			name: "namespace import",
			rule: "architecture/broad-import",
			files: {
				"modules/chat/view.ts":
					'import * as model from "./model";\nexport const value = model.value;\n',
				"modules/chat/model.ts": "export const value = 1;\n",
			},
		},
		{
			name: "raw useEffect",
			rule: "react/raw-use-effect",
			files: {
				"surfaces/chat/view.tsx":
					'import { useEffect } from "react";\nexport const View = () => { useEffect(() => {}); return null; };\n',
			},
		},
		{
			name: "browser API outside adapter",
			rule: "browser/adapter-only-api",
			files: {
				"modules/chat/model.ts":
					'export const load = () => fetch("/api/chat");\n',
			},
		},
		{
			name: "custom DOM event even inside adapter",
			rule: "browser/custom-event",
			files: {
				"adapters/events/client.ts":
					'export const event = () => new CustomEvent("oqto:test");\n',
			},
		},
		{
			name: "DOM query coordination",
			rule: "browser/dom-query-coordination",
			files: {
				"adapters/dom/client.ts":
					'export const find = () => document.querySelector("main");\n',
			},
		},
		{
			name: "component source-line budget",
			rule: "budget/source-lines",
			files: {
				"surfaces/large/View.tsx": `${Array.from({ length: 301 }, (_, index) => `const value${index} = ${index};`).join("\n")}\n`,
			},
		},
		{
			name: "public operation budget",
			rule: "budget/public-operations",
			files: {
				"modules/wide/model.ts": `${Array.from({ length: 8 }, (_, index) => `export function operation${index}() { return ${index}; }`).join("\n")}\n`,
			},
		},
		{
			name: "object-literal public interface budget",
			rule: "budget/public-operations",
			files: {
				"modules/wide/model.ts": `export const timeline = {\n${Array.from({ length: 8 }, (_, index) => `operation${index}: () => ${index},`).join("\n")}\n};\n`,
			},
		},
		{
			name: "exported interface operation budget",
			rule: "budget/public-operations",
			files: {
				"modules/wide/model.ts": `export interface Timeline {\n${Array.from({ length: 8 }, (_, index) => `operation${index}(): void;`).join("\n")}\n}\n`,
			},
		},
		{
			name: "exported property-function interface budget",
			rule: "budget/public-operations",
			files: {
				"modules/wide/model.ts": `export interface Timeline {\n${Array.from({ length: 8 }, (_, index) => `operation${index}: () => void;`).join("\n")}\n}\n`,
			},
		},
		{
			name: "exported type-alias operation budget",
			rule: "budget/public-operations",
			files: {
				"modules/wide/model.ts": `export type Timeline = {\n${Array.from({ length: 8 }, (_, index) => `operation${index}: () => void;`).join("\n")}\n};\n`,
			},
		},
		{
			name: "props budget",
			rule: "budget/props",
			files: {
				"surfaces/wide/View.tsx": `interface ViewProps {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\nexport const View = (_props: ViewProps) => null;\n`,
			},
		},
		{
			name: "renamed component props budget",
			rule: "budget/props",
			files: {
				"surfaces/wide/View.tsx": `interface ViewInput {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\nexport const View = (_input: ViewInput) => null;\n`,
			},
		},
		{
			name: "forwardRef component props budget",
			rule: "budget/props",
			files: {
				"surfaces/wide/View.tsx": `import { forwardRef } from "react";\ninterface ViewInput {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\nexport const View = forwardRef((_input: ViewInput, _ref) => null);\n`,
			},
		},
		{
			name: "imported component props budget",
			rule: "budget/props",
			files: {
				"surfaces/wide/View.tsx":
					'import type { ViewInput } from "./types";\nexport const View = (_input: ViewInput) => null;\n',
				"surfaces/wide/types.ts": `export interface ViewInput {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\n`,
			},
		},
		{
			name: "unstructured options bag",
			rule: "budget/unstructured-options",
			files: {
				"modules/options/model.ts":
					"export function run(options: Record<string, unknown>) { return options; }\n",
			},
		},
		{
			name: "renamed inline options bag",
			rule: "budget/unstructured-options",
			files: {
				"modules/options/model.ts":
					"export function run(params: { retries: number }) { return params; }\n",
			},
		},
		{
			name: "oversized named options bag",
			rule: "budget/options-fields",
			files: {
				"modules/options/model.ts": `interface RunOptions {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\nexport function run(options: RunOptions) { return options; }\n`,
			},
		},
		{
			name: "oversized imported options bag",
			rule: "budget/options-fields",
			files: {
				"modules/options/model.ts":
					'import type { RunOptions } from "./types";\nexport function run(options: RunOptions) { return options; }\n',
				"modules/options/types.ts": `export interface RunOptions {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\n`,
			},
		},
		{
			name: "hardcoded visual value",
			rule: "design/hardcoded-visual",
			files: {
				"surfaces/card/View.tsx":
					'export const View = () => <div className="shadow-lg bg-[#fff]" />;\n',
			},
		},
		{
			name: "inline style",
			rule: "design/inline-style",
			files: {
				"surfaces/card/View.tsx":
					"export const View = () => <div style={{ opacity: 1 }} />;\n",
			},
		},
		{
			name: "untranslated JSX text",
			rule: "i18n/untranslated-text",
			files: {
				"surfaces/chat/View.tsx":
					"export const View = () => <p>Start a new session</p>;\n",
			},
		},
		{
			name: "untranslated accessibility label",
			rule: "i18n/untranslated-attribute",
			files: {
				"surfaces/chat/View.tsx":
					'export const View = () => <button aria-label="Close session" />;\n',
			},
		},
	])("rejects $name", ({ files, rule }) => {
		expect(runFailure(files)).toContain(`[${rule}]`);
	});
});
