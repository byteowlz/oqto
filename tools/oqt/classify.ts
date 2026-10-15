// Trusted (guard-side) classification of coding tool calls into permission
// targets. The brain's own target claim is ignored: what a call may do is
// decided from the tool name and its arguments here.
import { basename } from "node:path";

export type Category = "read" | "edit" | "shell";

export interface Classified {
	category: Category;
	/** Ledger resource id (capability `execute`). */
	resource: string;
	/** Simple shell commands only: the remembered/approvable prefix. */
	prefix?: string;
	/** Short human preview for the approval menu. */
	preview: string;
}

// Commands whose first argument selects a subcommand worth keeping apart
// (`npm test` vs `npm publish`).
const SUBCOMMAND_TOOLS = new Set([
	"npm",
	"pnpm",
	"yarn",
	"bun",
	"npx",
	"cargo",
	"git",
	"just",
	"make",
	"go",
	"uv",
	"pip",
	"docker",
	"podman",
	"kubectl",
	"trx",
	"deno",
	"gh",
	"rustup",
	"mise",
	"pytest",
	"jj",
]);
// Commands that run another command: approving the wrapper would approve
// anything, so they are always generic shell.
const WRAPPERS = new Set([
	"sh",
	"bash",
	"zsh",
	"fish",
	"env",
	"sudo",
	"doas",
	"xargs",
	"eval",
	"exec",
	"timeout",
	"nice",
	"nohup",
	"time",
	"watch",
	"command",
	"builtin",
	"stdbuf",
	"script",
	"su",
]);

/** `npm test --watch` -> `npm test`; anything with shell syntax -> undefined. */
export function shellPrefix(command: string): string | undefined {
	const text = command.trim();
	// Sequencing, pipes, redirects, substitutions, subshells, escapes, newlines.
	if (!text || /[;&|<>`$(){}\\\n\r]/.test(text)) return undefined;
	const words = text.split(/\s+/);
	const first = words[0];
	if (!first || !/^[A-Za-z0-9._/@+-]+$/.test(first)) return undefined;
	if (first.includes("=")) return undefined; // VAR=x cmd
	const name = basename(first);
	if (WRAPPERS.has(name)) return undefined;
	const second = words[1];
	if (
		second !== undefined &&
		SUBCOMMAND_TOOLS.has(name) &&
		/^[a-z][a-z0-9:_-]*$/.test(second)
	)
		return `${first} ${second}`;
	return first;
}

function clip(text: string, lines: number, width = 160) {
	const all = text.split("\n");
	const shown = all
		.slice(0, lines)
		.map((line) => (line.length > width ? `${line.slice(0, width)}…` : line));
	if (all.length > lines) shown.push(`… (${all.length - lines} more lines)`);
	return shown.join("\n");
}

function editPreview(input: Record<string, unknown>) {
	const edits = Array.isArray(input.edits) ? input.edits : [input];
	const parts: string[] = [];
	for (const edit of edits.slice(0, 3)) {
		if (!edit || typeof edit !== "object") continue;
		const oldText = "oldText" in edit ? String(edit.oldText) : "";
		const newText = "newText" in edit ? String(edit.newText) : "";
		parts.push(
			clip(
				[
					...oldText.split("\n").map((line) => `- ${line}`),
					...newText.split("\n").map((line) => `+ ${line}`),
				].join("\n"),
				10,
			),
		);
	}
	if (edits.length > 3) parts.push(`… ${edits.length - 3} more edits`);
	return parts.join("\n");
}

export function classify(tool: string, input: unknown): Classified {
	const args = (input && typeof input === "object" ? input : {}) as Record<
		string,
		unknown
	>;
	const path = typeof args.path === "string" ? args.path : "";
	switch (tool) {
		case "read":
			return {
				category: "read",
				resource: "workspace-read",
				preview: `Read ${path}`,
			};
		case "write": {
			const content = typeof args.content === "string" ? args.content : "";
			return {
				category: "edit",
				resource: "workspace-write",
				preview: `Write ${path} (${content.split("\n").length} lines)\n${clip(content, 8)}`,
			};
		}
		case "edit":
			return {
				category: "edit",
				resource: "workspace-write",
				preview: `Edit ${path}\n${editPreview(args)}`,
			};
		case "bash": {
			const command = typeof args.command === "string" ? args.command : "";
			const prefix = shellPrefix(command);
			return {
				category: "shell",
				resource: prefix ? `shell:${prefix}` : "workspace-shell",
				...(prefix ? { prefix } : {}),
				preview: `$ ${clip(command, 12)}`,
			};
		}
		default:
			throw new Error(`Unclassified tool ${tool}; refusing admission`);
	}
}
