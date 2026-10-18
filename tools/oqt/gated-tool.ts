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
	budget?(operation: CodingOperation): number;
	drain?(operation: CodingOperation): Promise<void>;
	/** Execution env pinned to the generation admitted for this operation.
	 * A revoke closes it; the tool then fails instead of moving to new hands. */
	env?(operation: CodingOperation): unknown;
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
		executionMode: "sequential",
		async execute(args, api, context) {
			const operation: CodingOperation = {
				request_id: `${publicSession}:store:${api.conversationId}:${api.taskId}`,
				target: { capability: "execute", resource_id: "workspace-tools" },
				arguments: { tool: tool.name, input: structuredClone(args) },
			};
			let admitted = false,
				drainAttempted = false,
				drained = false;
			let timer: ReturnType<typeof setTimeout> | undefined;
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
						if (key === "env" && gate.env) return gate.env(operation);
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
				const budget = Math.min(30_000, gate.budget?.(operation) ?? 30_000);
				if (budget <= 0) throw new Error("Invocation lease ended");
				const result = await Promise.race([
					tool.execute(args, buffered, context),
					new Promise<never>((_resolve, reject) => {
						timer = setTimeout(
							() => reject(new Error("Invocation deadline")),
							budget,
						);
					}),
				]);
				clearTimeout(timer);
				drainAttempted = true;
				await gate.drain?.(operation);
				drained = true;
				text += decoder.decode();
				context.abortSignal?.throwIfAborted();
				await gate.check(operation, context);
				return {
					...result,
					...(result.output === undefined && text !== ""
						? { output: [{ type: "text" as const, text }] }
						: {}),
				};
			} catch (error) {
				// Provider exceptions may contain data. Never disclose those on a
				// failed gate, timeout, cancellation, or uncertain effect.
				if (process.env.OQT_DEBUG) console.error("oqt tool failure:", error);
				throw new Error(
					admitted
						? "Confined operation failed or authorization ended; effects may be uncertain. Inspect before retrying."
						: "Permission denied or unavailable; confined operation was not started.",
				);
			} finally {
				clearTimeout(timer);
				if (admitted) {
					if (!drainAttempted) {
						await gate.drain?.(operation);
						drained = true;
					}
					// Never erase evidence while descendants may still be running.
					if (drained) await gate.finish(operation);
				}
			}
		},
	};
}
