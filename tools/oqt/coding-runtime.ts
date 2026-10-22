import { resolve } from "node:path";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import {
	type Models,
	type ModelThinkingLevel,
	Type,
} from "@earendil-works/pi-ai";
import {
	createRegistry,
	defineExtension,
	Harness,
	type ModelRef,
	type PromptInput,
	section,
} from "@earendil-works/pi-durable";
import type { ExecutionEnv } from "@earendil-works/pi-durable/env";
import { openNodeSqliteStorage } from "@earendil-works/pi-durable/storage/sqlite/node";
import { createCodingTools } from "@earendil-works/pi-durable/tools";
import type { GuardGate } from "./brain-client.ts";
import { readClassicImportSeed } from "./classic-import-host.ts";
import { applyClassicImport } from "./classic-import-worker.ts";
import { type CodingGate, gatedTool } from "./gated-tool.ts";
import { permissionFeedback, permissionFeedbackText } from "./permission-feedback.ts";
import type { HandsClient } from "./hands-client.ts";

/** Confinement and permission rules the model must know (prompt addendum). */
export const OQT_RULES =
	"You are a coding assistant using confined executors limited to the workspace, without network. Every coding call needs local-operator approval. Use request_access to ask for a specific outside path or for network, with a reason; never try to work around a denial. Protected runtime, credentials, and authority are unavailable. Interrupted effects are uncertain: inspect before retrying. No skills, codemode, subagents, or unrestricted execution are available.";

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
	/** Guard-mediated external resource requests (operator-approved mounts). */
	requestAccess?: GuardGate["requestAccess"];
	/** Full system prompt sections (pi's structure); falls back to the oqt rules only. */
	promptSections?: (input: PromptInput) => Record<string, string>;
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
	const requestAccess = options.requestAccess;
	if (requestAccess)
		registry.install(
			defineExtension({
				name: "oqt-resource-requests",
				tools: [
					{
						name: "request_access",
						description:
							"Ask the local operator for access outside the workspace: an existing path (read or write) or outbound network. Explain why. The operator may deny. Once granted, use the normal tools with absolute paths; each tool call is still approved separately. Grants are Session-scoped and expire.",
						parameters: Type.Object({
							path: Type.Optional(
								Type.String({
									description: "Absolute or workspace-relative existing path",
								}),
							),
							access: Type.Optional(
								Type.Union([Type.Literal("read"), Type.Literal("write")]),
							),
							network: Type.Optional(
								Type.Boolean({
									description: "Request outbound network for tools",
								}),
							),
							reason: Type.String({ description: "Why the access is needed" }),
						}),
						replay: "unsafe",
						executionMode: "sequential",
						async execute(args, api, context) {
							const { path, access, network, reason } = args as {
								path?: string;
								access?: "read" | "write";
								network?: boolean;
								reason: string;
							};
							if ((path === undefined) === (network !== true))
								throw new Error(
									"Request either a path (with access) or network",
								);
							try {
								const result = await requestAccess(
									{
										request_id: `${options.session}:store:${api.conversationId}:${api.taskId}`,
										...(path === undefined
											? {}
											: { path, access: access ?? "read" }),
										...(network ? { network: true } : {}),
										reason,
									},
									context.abortSignal,
								);
								return {
									details: {
										granted: { capability: result.granted.capability, resource: result.granted.resource },
										expiresAt: result.expires_at,
									},
									output: [
										{
											type: "text" as const,
											text: `Granted ${result.granted.capability} access to ${result.granted.resource} until ${new Date(result.expires_at).toISOString()}. Use absolute paths with the normal tools; each call is still approved.`,
										},
									],
								};
							} catch (error) {
								const feedback = permissionFeedback(error);
								// UI details are fixed, privacy-safe admission metadata, not tool output.
								// Preserve native failure semantics and durable result diagnostics.
								if (typeof api.details === "function" && !context.abortSignal?.aborted) {
									try { await api.details({ permissionFeedback: feedback }, context); }
									catch { /* Presentation failure must not replace the admission cause. */ }
								}
								throw new Error(permissionFeedbackText(feedback));
							}
						},
					},
				],
			}),
		);
	const promptSections = options.promptSections;
	if (promptSections) {
		// pi's section order, plus the oqt rules as the addendum. One build per request.
		const keys = [
			"preamble",
			"tools",
			"rules",
			"docs",
			"addendum",
			"project_context",
			"skills",
			"cwd",
		] as const;
		const built = new WeakMap<PromptInput, Record<string, string>>();
		const build = (input: PromptInput) => {
			let sections = built.get(input);
			if (!sections) {
				sections = promptSections(input);
				built.set(input, sections);
			}
			return sections;
		};
		registry.install(
			defineExtension({
				name: "oqt-pi-prompt",
				sections: keys.map((key) =>
					section(key, (input) => build(input)[key], { tag: false }),
				),
			}),
		);
	} else
		registry.install(
			defineExtension({
				name: "oqt-protected-coding-prompt",
				sections: [{ key: "oqt", render: () => OQT_RULES }],
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
		// The factory has not exposed services or resumed the scheduler yet.
		// Root policy is owner-approved; imported settings never select execution policy.
		const importSeed = await readClassicImportSeed();
		if (importSeed)
			await applyClassicImport(harness, importSeed, BACKGROUND_CONTEXT);
		return { harness, conversation };
	} catch (error) {
		await harness.close(BACKGROUND_CONTEXT);
		throw error;
	}
}
