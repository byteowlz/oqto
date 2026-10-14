import type { Context } from "@earendil-works/chord";
import type {
	ToolExecutionApi,
	ToolRegistration,
} from "@earendil-works/pi-durable";

export interface CodingOperation {
	request_id: string;
	target: { capability: "execute"; resource_id: "workspace-tools" };
	arguments: { tool: string; input: unknown };
}

/** Owner-provided bridge. A model/tool cannot supply or replace this gate. */
export interface CodingGate {
	prepare(operation: CodingOperation, context: Context): Promise<void>;
	check(operation: CodingOperation, context: Context): Promise<void>;
	finish(operation: CodingOperation): Promise<void>;
}

/** Withhold ALL progress until delivery authorization, including failures.
 * Streaming API output is synchronous, so an asynchronous ledger check cannot
 * protect each call to output(). Buffer instead; never commit unchecked data.
 * No unsafe execution is automatically replayed after a worker crash.
 */
export function gatedTool(
	tool: ToolRegistration,
	gate: CodingGate,
	publicSession: string,
): ToolRegistration {
	return {
		...tool,
		replay: "unsafe",
		async execute(args, api, context) {
			const operation: CodingOperation = {
				request_id: `${publicSession}:store:${api.conversationId}:${api.taskId}`,
				target: { capability: "execute", resource_id: "workspace-tools" },
				arguments: { tool: tool.name, input: structuredClone(args) },
			};
			let admitted = false;
			try {
				context.abortSignal?.throwIfAborted();
				await gate.prepare(operation, context);
				admitted = true;
				await gate.check(operation, context);
				let text = "";
				const decoder = new TextDecoder();
				const buffered = new Proxy(api, {
					get(target, key) {
						if (key === "outputWindow") return undefined;
						if (key === "output")
							return (chunk: string | Uint8Array, skipped?: unknown) => {
								if (skipped !== undefined)
									throw new Error("Unexpected skipped confined output");
								text +=
									typeof chunk === "string"
										? chunk
										: decoder.decode(chunk, { stream: true });
								if (Buffer.byteLength(text) > 1024 * 1024)
									throw new Error("Confined output exceeds delivery buffer");
							};
						if (key === "retainedOutput")
							return () => ({ text, truncated: false });
						if (key === "diagnostic") return () => {};
						if (key === "details") return async () => {};
						if (["commit", "createTask", "executeTool"].includes(String(key)))
							return () => {
								throw new Error(
									"Coding tool attempted an unmediated task effect",
								);
							};
						const value = Reflect.get(target, key, target);
						return typeof value === "function" ? value.bind(target) : value;
					},
				}) as ToolExecutionApi;
				const result = await tool.execute(args, buffered, context);
				text += decoder.decode();
				context.abortSignal?.throwIfAborted();
				await gate.check(operation, context);
				return {
					...result,
					...(result.output === undefined && text !== ""
						? { output: [{ type: "text" as const, text }] }
						: {}),
				};
			} catch {
				// Provider exceptions may contain data. Never disclose those on a
				// failed gate, timeout, cancellation, or uncertain effect.
				throw new Error(
					admitted
						? "Confined operation failed or authorization ended; effects may be uncertain. Inspect before retrying."
						: "Permission denied or unavailable; confined operation was not started.",
				);
			} finally {
				if (admitted) await gate.finish(operation);
			}
		},
	};
}
