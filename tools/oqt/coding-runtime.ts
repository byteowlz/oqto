import { resolve } from "node:path";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import type { Models, ModelThinkingLevel } from "@earendil-works/pi-ai";
import {
	createRegistry,
	defineExtension,
	Harness,
	type ModelRef,
} from "@earendil-works/pi-durable";
import type { ExecutionEnv } from "@earendil-works/pi-durable/env";
import { openNodeSqliteStorage } from "@earendil-works/pi-durable/storage/sqlite/node";
import { createCodingTools } from "@earendil-works/pi-durable/tools";
import { type CodingGate, gatedTool } from "./gated-tool.ts";
import type { HandsClient } from "./hands-client.ts";

/** Brain-side protected coding composition. No NodeExecutionEnv, skills,
 * shell config commands, codemode, or unreviewed plugin registry. Authority
 * and hands are reached only through the supplied guard gate/client. */
export async function openCodingHarness(options: {
	databasePath: string;
	session: string;
	workspace: string;
	models: Models;
	model: ModelRef;
	thinkingLevel?: ModelThinkingLevel;
	gate: CodingGate;
	hands: HandsClient;
	/** Adopt the guard's current hands generation before harness-internal access. */
	refresh: () => Promise<void>;
}) {
	const workspace = resolve(options.workspace);
	const registry = createRegistry();
	const coding = createCodingTools();
	registry.install({
		...coding,
		tools: coding.tools?.map((tool) =>
			gatedTool(tool, options.gate, options.session),
		),
	});
	registry.install(
		defineExtension({
			name: "oqt-protected-coding-prompt",
			sections: [
				{
					key: "oqt",
					render: () =>
						"You are a coding assistant using confined executors. Every coding call needs local-operator approval. Protected runtime, credentials, and authority are unavailable to tools. Never bypass a denial. Interrupted effects are uncertain: inspect before retrying. No skills, codemode, subagents, or unrestricted execution are available.",
				},
			],
		}),
	);
	const execution = new Proxy({} as ExecutionEnv, {
		get(_target, key) {
			if (key === "cwd") return workspace;
			// One stable namespace: every generation sees the same workspace paths.
			if (key === "id") return "oqt-guarded-workspace";
			// Hands are guard-owned; brain shutdown must not try to manage them.
			if (key === "cleanup") return async () => {};
			if (key === "then") return undefined; // not a thenable
			// Harness-internal access only (e.g. context files); tool calls use the
			// env pinned at admission by the gate. Resolved per call.
			return async (...args: unknown[]) => {
				await options.refresh();
				const current = options.hands.current as unknown as Record<
					PropertyKey,
					unknown
				>;
				const value = current[key];
				if (typeof value !== "function")
					throw new Error(`Unsupported execution property ${String(key)}`);
				return value.apply(current, args);
			};
		},
	});
	const harness = await Harness.open(
		await openNodeSqliteStorage(options.databasePath),
		{
			models: options.models,
			registry,
			env(target) {
				if (!target.cwd || resolve(target.cwd) !== workspace)
					throw new Error(
						"Conversation attempted to change the owner-bound workspace",
					);
				return execution;
			},
		},
		BACKGROUND_CONTEXT,
	);
	try {
		const conversation = await harness.root(BACKGROUND_CONTEXT, {
			agent: {
				cwd: workspace,
				model: options.model,
				...(options.thinkingLevel
					? { thinkingLevel: options.thinkingLevel }
					: {}),
			},
		});
		return { harness, conversation };
	} catch (error) {
		await harness.close(BACKGROUND_CONTEXT);
		throw error;
	}
}
