import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const SCRIPT = path.resolve("scripts/check-oqto-ui-guardrails.mjs");
const tempRoots: string[] = [];

function fixture(files: Record<string, string>): string {
	const root = mkdtempSync(path.join(tmpdir(), "oqto-ui-guardrails-"));
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

describe("OqtoUI architecture guardrails", () => {
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
			"app/main/route.ts":
				'import { shell } from "../../chat/shell/view";\nexport const route = () => shell();\n',
			"chat/shell/view.ts":
				'import { tabs } from "../tabs/model";\nexport const shell = () => tabs();\n',
			"chat/tabs/model.ts":
				'import { readLayout } from "../../platform/layout/storage";\nexport const tabs = () => readLayout();\n',
			"platform/layout/storage.ts":
				'export const readLayout = () => localStorage.getItem("oqto:layout");\n',
		});
		expect(() =>
			execFileSync(process.execPath, [SCRIPT, "--source-root", root]),
		).not.toThrow();
	});

	it("accepts internal unknown narrowing and adapter-boundary unknown", () => {
		const root = fixture({
			"chat/data/model.ts":
				'export function parse(text: string): string {\n\tconst raw: unknown = JSON.parse(text);\n\treturn typeof raw === "string" ? raw : "";\n}\n',
			"platform/config/parse.ts":
				"export function narrow(value: unknown): string {\n\treturn typeof value === 'string' ? value : '';\n}\n",
		});
		expect(() =>
			execFileSync(process.execPath, [SCRIPT, "--source-root", root]),
		).not.toThrow();
	});

	it("accepts react-i18next Trans fallback content", () => {
		const root = fixture({
			"sessions/chat/View.tsx":
				'import { Trans } from "react-i18next";\nexport const View = () => <Trans i18nKey="chat.start">Start a <strong>new session</strong></Trans>;\n',
		});
		expect(() =>
			execFileSync(process.execPath, [SCRIPT, "--source-root", root]),
		).not.toThrow();
	});

	it("rejects an exception not approved by the project owner", () => {
		const root = fixture({
			"sessions/chat/View.tsx":
				"export const View = () => <p>Unapproved text</p>;\n",
			"exceptions.json": JSON.stringify({
				version: 1,
				exceptions: [
					{
						rule: "i18n/untranslated-text",
						file: "sessions/chat/View.tsx",
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
		expect(result.stderr).toContain("owner approval");
	});

	it("honors only an exact owner-approved exception manifest entry", () => {
		const root = fixture({
			"sessions/chat/View.tsx":
				"export const View = () => <p>Temporary approved text</p>;\n",
			"exceptions.json": JSON.stringify({
				version: 1,
				exceptions: [
					{
						rule: "i18n/untranslated-text",
						file: "sessions/chat/View.tsx",
						line: 1,
						reason: "Owner approved this fixture exception",
						removalCondition: "Remove when the fixture test completes",
						issue: "oqto-2jjm.1",
						approvedBy: "owner",
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
				"sessions/chat/view.ts":
					'import { useApp } from "@/hooks/use-app";\nexport const view = useApp;\n',
			},
		},
		{
			name: "relative import escaping the Workbench boundary",
			rule: "architecture/source-boundary-import",
			files: {
				"platform/chat/client.ts":
					'import { useApp } from "../../../hooks/use-app";\nexport const client = useApp;\n',
			},
		},
		{
			name: "legacy feature barrel import",
			rule: "architecture/legacy-import",
			files: {
				"sessions/chat/view.ts":
					'import { ChatView } from "@/features/chat";\nexport const view = ChatView;\n',
			},
		},
		{
			name: "unapproved project alias",
			rule: "architecture/project-alias-import",
			files: {
				"platform/apps/registry.ts":
					'import { appRegistry } from "@/lib/app-registry";\nexport const registry = appRegistry;\n',
			},
		},
		{
			name: "reverse layer dependency",
			rule: "architecture/layer-direction",
			files: {
				"chat/chat/model.ts":
					'import { view } from "../../sessions/chat/view";\nexport const model = view;\n',
				"sessions/chat/view.ts": "export const view = 1;\n",
			},
		},
		{
			name: "cross-feature dependency",
			rule: "architecture/layer-direction",
			files: {
				"chat/model.ts":
					'import { files } from "../files/model";\nexport const chat = files;\n',
				"files/model.ts": "export const files = 1;\n",
			},
		},
		{
			name: "import cycle",
			rule: "architecture/import-cycle",
			files: {
				"chat/chat/a.ts": 'import { b } from "./b";\nexport const a = b;\n',
				"chat/chat/b.ts": 'import { a } from "./a";\nexport const b = a;\n',
			},
		},
		{
			name: "broad export-star barrel",
			rule: "architecture/broad-barrel",
			files: {
				"chat/chat/index.ts": 'export * from "./model";\n',
				"chat/chat/model.ts": "export const value = 1;\n",
			},
		},
		{
			name: "namespace export barrel",
			rule: "architecture/broad-barrel",
			files: {
				"chat/chat/index.ts": 'export * as model from "./model";\n',
				"chat/chat/model.ts": "export const value = 1;\n",
			},
		},
		{
			name: "namespace import",
			rule: "architecture/broad-import",
			files: {
				"chat/chat/view.ts":
					'import * as model from "./model";\nexport const value = model.value;\n',
				"chat/chat/model.ts": "export const value = 1;\n",
			},
		},
		{
			name: "raw useEffect",
			rule: "react/raw-use-effect",
			files: {
				"sessions/chat/view.tsx":
					'import { useEffect } from "react";\nexport const View = () => { useEffect(() => {}); return null; };\n',
			},
		},
		{
			name: "browser API outside adapter",
			rule: "browser/adapter-only-api",
			files: {
				"chat/chat/model.ts": 'export const load = () => fetch("/api/chat");\n',
			},
		},
		{
			name: "custom DOM event even inside adapter",
			rule: "browser/custom-event",
			files: {
				"platform/events/client.ts":
					'export const event = () => new CustomEvent("oqto:test");\n',
			},
		},
		{
			name: "DOM query coordination",
			rule: "browser/dom-query-coordination",
			files: {
				"platform/dom/client.ts":
					'export const find = () => document.querySelector("main");\n',
			},
		},
		{
			name: "component source-line budget",
			rule: "budget/source-lines",
			files: {
				"sessions/large/View.tsx": `${Array.from({ length: 301 }, (_, index) => `const value${index} = ${index};`).join("\n")}\n`,
			},
		},
		{
			name: "public operation budget",
			rule: "budget/public-operations",
			files: {
				"chat/wide/model.ts": `${Array.from({ length: 8 }, (_, index) => `export function operation${index}() { return ${index}; }`).join("\n")}\n`,
			},
		},
		{
			name: "object-literal public interface budget",
			rule: "budget/public-operations",
			files: {
				"chat/wide/model.ts": `export const timeline = {\n${Array.from({ length: 8 }, (_, index) => `operation${index}: () => ${index},`).join("\n")}\n};\n`,
			},
		},
		{
			name: "exported interface operation budget",
			rule: "budget/public-operations",
			files: {
				"chat/wide/model.ts": `export interface Timeline {\n${Array.from({ length: 8 }, (_, index) => `operation${index}(): void;`).join("\n")}\n}\n`,
			},
		},
		{
			name: "exported property-function interface budget",
			rule: "budget/public-operations",
			files: {
				"chat/wide/model.ts": `export interface Timeline {\n${Array.from({ length: 8 }, (_, index) => `operation${index}: () => void;`).join("\n")}\n}\n`,
			},
		},
		{
			name: "exported type-alias operation budget",
			rule: "budget/public-operations",
			files: {
				"chat/wide/model.ts": `export type Timeline = {\n${Array.from({ length: 8 }, (_, index) => `operation${index}: () => void;`).join("\n")}\n};\n`,
			},
		},
		{
			name: "props budget",
			rule: "budget/props",
			files: {
				"sessions/wide/View.tsx": `interface ViewProps {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\nexport const View = (_props: ViewProps) => null;\n`,
			},
		},
		{
			name: "renamed component props budget",
			rule: "budget/props",
			files: {
				"sessions/wide/View.tsx": `interface ViewInput {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\nexport const View = (_input: ViewInput) => null;\n`,
			},
		},
		{
			name: "forwardRef component props budget",
			rule: "budget/props",
			files: {
				"sessions/wide/View.tsx": `import { forwardRef } from "react";\ninterface ViewInput {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\nexport const View = forwardRef((_input: ViewInput, _ref) => null);\n`,
			},
		},
		{
			name: "imported component props budget",
			rule: "budget/props",
			files: {
				"sessions/wide/View.tsx":
					'import type { ViewInput } from "./types";\nexport const View = (_input: ViewInput) => null;\n',
				"sessions/wide/types.ts": `export interface ViewInput {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\n`,
			},
		},
		{
			name: "unstructured options bag",
			rule: "budget/unstructured-options",
			files: {
				"chat/options/model.ts":
					"export function run(options: Record<string, unknown>) { return options; }\n",
			},
		},
		{
			name: "renamed inline options bag",
			rule: "budget/unstructured-options",
			files: {
				"chat/options/model.ts":
					"export function run(params: { retries: number }) { return params; }\n",
			},
		},
		{
			name: "oversized named options bag",
			rule: "budget/options-fields",
			files: {
				"chat/options/model.ts": `interface RunOptions {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\nexport function run(options: RunOptions) { return options; }\n`,
			},
		},
		{
			name: "oversized imported options bag",
			rule: "budget/options-fields",
			files: {
				"chat/options/model.ts":
					'import type { RunOptions } from "./types";\nexport function run(options: RunOptions) { return options; }\n',
				"chat/options/types.ts": `export interface RunOptions {\n${Array.from({ length: 9 }, (_, index) => `field${index}: string;`).join("\n")}\n}\n`,
			},
		},
		{
			name: "Record with unknown values",
			rule: "types/record-unknown",
			files: {
				"chat/data/model.ts":
					"export type Payload = { fields: Record<string, unknown> };\nexport function read(payload: Payload) { return payload; }\n",
			},
		},
		{
			name: "index signature with any values",
			rule: "types/record-unknown",
			files: {
				"chat/data/model.ts":
					"export interface Bag {\n\t[key: string]: any;\n}\nexport function read(bag: Bag) { return bag; }\n",
			},
		},
		{
			name: "non-Record container with string-unknown arguments",
			rule: "types/record-unknown",
			files: {
				"chat/data/model.ts":
					"export type Cache = Map<string, unknown>;\nexport function read(cache: Cache) { return cache; }\n",
			},
		},
		{
			name: "exported unknown parameter outside platform adapters",
			rule: "types/exported-unknown",
			files: {
				"chat/data/model.ts":
					"export function parse(value: unknown) { return String(value); }\n",
			},
		},
		{
			name: "exported unknown type alias outside platform adapters",
			rule: "types/exported-unknown",
			files: {
				"chat/data/model.ts":
					"export type Loose = { value: unknown };\nexport function read(input: Loose) { return input; }\n",
			},
		},
		{
			name: "hardcoded visual value",
			rule: "design/hardcoded-visual",
			files: {
				"sessions/card/View.tsx":
					'export const View = () => <div className="shadow-lg bg-[#fff]" />;\n',
			},
		},
		{
			name: "inline style",
			rule: "design/inline-style",
			files: {
				"sessions/card/View.tsx":
					"export const View = () => <div style={{ opacity: 1 }} />;\n",
			},
		},
		{
			name: "untranslated JSX text",
			rule: "i18n/untranslated-text",
			files: {
				"sessions/chat/View.tsx":
					"export const View = () => <p>Start a new session</p>;\n",
			},
		},
		{
			name: "untranslated accessibility label",
			rule: "i18n/untranslated-attribute",
			files: {
				"sessions/chat/View.tsx":
					'export const View = () => <button aria-label="Close session" />;\n',
			},
		},
	])("rejects $name", ({ files, rule }) => {
		expect(runFailure(files)).toContain(`[${rule}]`);
	});
});
