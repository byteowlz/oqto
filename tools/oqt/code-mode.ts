// Code-mode fan-out (ADR-0079 amendment item 1, ADR-0086 invariant 7). A
// JavaScript cell runs in QuickJS inside the confined brain; its only
// capability is calling the gated coding tools. Every call is a nested
// Durable tool task through the Harness (`executeTool`), so it is admitted,
// approved, journaled and receipted exactly like a direct call and keeps its
// `machine` argument. Nothing is retried: not a call, not the cell.
import { Type } from "@earendil-works/pi-ai";
import {
	type CodemodeJsonSchema,
	InlineCodemodeSandbox,
	loadQuickJSWasm,
} from "@earendil-works/pi-codemode";
import type { JsonObject, ToolRegistration } from "@earendil-works/pi-durable";
import type { MachineInfo } from "./machine-tools.ts";

export interface CellCall {
	index: number;
	tool: string;
	machine: string;
	status: "pending" | "ok" | "error";
	task?: string;
}

const CELL_OUTPUT_LIMIT = 64 * 1024;

export function codeModeTool(
	tools: readonly ToolRegistration[],
	machines: readonly MachineInfo[],
): ToolRegistration {
	const list = machines.map((m) => m.machine).join(", ");
	const names = tools.map((tool) => `tools.${tool.name}(args)`).join(", ");
	return {
		name: "code",
		description: `Run a JavaScript cell whose only capability is calling the coding tools: ${names}. Each call takes that tool's normal arguments plus an optional machine (${list || "none configured"}) and is admitted and approved like a direct call, with its own receipt. Calls start one at a time in call order; use Promise.allSettled so one failed or unknown call does not hide the others. A call resolves to its text output and rejects with its error text. Return a value or use text() / console.log. Calls and cells are never retried; inspect before repeating.`,
		parameters: Type.Object({
			code: Type.String({
				description: "JavaScript; top-level await allowed",
			}),
		}),
		callers: ["model"],
		replay: "unsafe",
		executionMode: "sequential",
		async execute(args, api, context) {
			const { code } = args as { code: string };
			const calls: CellCall[] = [];
			// Nested calls start one at a time in call order: the guard runs
			// effects on one lane anyway, and concurrent operator prompts on
			// the same ledger are not qualified (a sibling's decision can make
			// a pending admission stale). Settlement stays independent.
			let queue: Promise<unknown> = Promise.resolve();
			const sandbox = new InlineCodemodeSandbox({
				wasm: loadQuickJSWasm(),
				timeoutMs: 120_000,
				tools: tools.map((tool) => ({
					name: tool.name,
					description: tool.description,
					inputSchema: tool.parameters as unknown as CodemodeJsonSchema,
					async execute(input: unknown) {
						const machine =
							input && typeof input === "object" && "machine" in input
								? String((input as { machine: unknown }).machine)
								: "local";
						const entry: CellCall = {
							index: calls.length,
							tool: tool.name,
							machine,
							status: "pending",
						};
						calls.push(entry);
						const started = queue.then(() =>
							api.executeTool(tool.name, (input ?? {}) as JsonObject, context),
						);
						queue = started.catch(() => {});
						const nested = await started;
						entry.task = String(nested.taskId);
						entry.status = nested.isError ? "error" : "ok";
						const value = nested.structuredOutput;
						const text =
							typeof value === "string" ? value : JSON.stringify(value ?? "");
						if (nested.isError) throw new Error(text || `${tool.name} failed`);
						return text;
					},
				})),
			});
			const result = await sandbox.execute(code, {
				signal: context.abortSignal,
			});
			const printed = result.output
				.map((item) =>
					item.type === "text" ? item.text : `[image ${item.mimeType}]`,
				)
				.join("\n");
			const value =
				result.ok && result.value !== undefined
					? `\nResult: ${JSON.stringify(result.value)}`
					: "";
			const failure = result.ok
				? ""
				: `\nCell failed (${result.error.kind}): ${result.error.message}`;
			const summary = calls
				.map(
					(call) =>
						`#${call.index} ${call.tool} on ${call.machine}: ${call.status}`,
				)
				.join("\n");
			let text = `${printed}${value}${failure}\nCalls (one receipt each, none retried):\n${summary || "none"}`;
			if (text.length > CELL_OUTPUT_LIMIT)
				text = `${text.slice(0, CELL_OUTPUT_LIMIT)}\n[truncated]`;
			return {
				output: [{ type: "text" as const, text }],
				details: {
					calls: calls.map((call) => ({ ...call })),
				} as unknown as never,
				...(result.ok ? {} : { isError: true }),
			};
		},
	};
}
