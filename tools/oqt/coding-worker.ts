// Brain composition, copied beside the upstream experimental session worker in
// the private runtime and executed only inside the oqt_brain sandbox. It holds
// no authority: approvals, hands, the ledger and owner credentials live in the
// guard, reached through the per-session tool socket.

import { createStaticFacetLoader, defineFacet } from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import { ModelRuntime } from "../core/model-runtime.ts";
import { buildSystemPromptSections } from "../core/system-prompt.ts";
import { bashToolSystemPromptContribution } from "../core/tools/bash.ts";
import { editToolSystemPromptContribution } from "../core/tools/edit.ts";
import { readToolSystemPromptContribution } from "../core/tools/read.ts";
import { writeToolSystemPromptContribution } from "../core/tools/write.ts";
import { GuardGate, guardCredentials } from "./brain-client.ts";
import { OQT_RULES, openCodingHarness } from "./coding-runtime.ts";
import { WireClient } from "./guard-wire.ts";
import { HandsClient } from "./hands-client.ts";
import {
	installOperatorReceipts,
	OperatorResults,
} from "./operator-receipts.ts";
import { ownerModelsStore } from "./owner-models.ts";
import type { SessionWorkerRuntime } from "./services/worker.ts";
import { createSessionRouter } from "./session-router.ts";
import type { runSessionWorkerWithHarness } from "./session-worker.ts";

type Options = Parameters<Parameters<typeof runSessionWorkerWithHarness>[1]>[1];
// OQT_DEBUG=1 records startup phases in the private brain log.
const started = Date.now();
const debug = process.env.OQT_DEBUG
	? (phase: string) =>
			console.error(
				`oqt brain +${Date.now() - started}ms @${Date.now()} (origin @${Math.round(performance.timeOrigin)}): ${phase}`,
			)
	: () => {};
function required(name: string) {
	const value = process.env[name];
	if (!value) throw new Error(`Missing protected coding context: ${name}`);
	return value;
}

export async function createCodingHarness(
	databasePath: string,
	options: Options,
): Promise<SessionWorkerRuntime> {
	const defaults: unknown = JSON.parse(required("OQT_OWNER_DEFAULTS"));
	if (
		!defaults ||
		typeof defaults !== "object" ||
		!("provider" in defaults) ||
		typeof defaults.provider !== "string" ||
		!("model" in defaults) ||
		typeof defaults.model !== "string"
	)
		throw new Error("Invalid owner defaults from guard");
	debug("connecting guard");
	const guard = await WireClient.connect(required("OQT_GUARD_SOCKET"));
	debug("guard connected");
	const identity = await guard.call("identity");
	if (
		!identity ||
		typeof identity !== "object" ||
		!("pid" in identity) ||
		typeof identity.pid !== "number"
	)
		throw new Error("Guard did not attest the brain host pid");
	process.env.OQT_HOST_PID = String(identity.pid);
	const hands = new HandsClient(
		required("OQT_HANDS_SOCKET"),
		options.metadata.cwd,
	);
	try {
		// Read-only binds of the owner's catalog files; auth.json is not visible here.
		const owner = process.env.OQT_OWNER_AGENT_DIR;
		const modelRuntime = await ModelRuntime.create({
			credentials: guardCredentials(guard),
			modelsPath: process.env.OQT_OWNER_MODELS ?? null,
			...(owner ? { modelsStore: ownerModelsStore(owner) } : {}),
			allowModelNetwork: false,
		});
		debug("model runtime ready");
		const model = modelRuntime
			.getAllModels(defaults.provider)
			.find((candidate) => candidate.id === defaults.model);
		if (!model)
			throw new Error(
				`Owning model is unavailable: ${defaults.provider}/${defaults.model}; no default fallback`,
			);
		// Resolve auth now (the guard renews OAuth if near expiry) so failures
		// surface at startup rather than mid-turn.
		if (!(await modelRuntime.getAuth(model)))
			throw new Error(
				"Owning model credentials unavailable; login/renew through owning Pi",
			);
		debug("owner auth resolved");
		const levels = [
			"off",
			"minimal",
			"low",
			"medium",
			"high",
			"xhigh",
		] as const;
		const requested =
			"thinking" in defaults && typeof defaults.thinking === "string"
				? defaults.thinking
				: undefined;
		const thinking = levels.find((level) => level === requested);
		if (requested !== undefined && thinking === undefined)
			throw new Error("Unsupported owning thinking level");
		// pi's own system prompt; the trusted host supplies the context files.
		const contextFiles = await guard.call("context");
		if (!Array.isArray(contextFiles))
			throw new Error("Guard returned invalid context files");
		debug(`context files: ${contextFiles.length}`);
		const contributions = {
			read: readToolSystemPromptContribution,
			bash: bashToolSystemPromptContribution,
			edit: editToolSystemPromptContribution,
			write: writeToolSystemPromptContribution,
		};
		const promptSections = (input: {
			agent: { tools: readonly { name: string }[] };
		}) => {
			const selectedTools = input.agent.tools.map((tool) => tool.name);
			const toolSnippets: Record<string, string> = {};
			const toolGuidelines: Record<string, string[]> = {};
			for (const name of selectedTools) {
				const contribution = contributions[name as keyof typeof contributions];
				if (!contribution) continue;
				toolSnippets[name] = contribution.snippet;
				toolGuidelines[name] = [...contribution.guidelines];
			}
			return buildSystemPromptSections({
				cwd: options.metadata.cwd,
				selectedTools,
				toolSnippets,
				toolGuidelines,
				appendSystemPrompt: OQT_RULES,
				contextFiles: contextFiles as { path: string; content: string }[],
				skills: [],
			}) as Record<string, string>;
		};
		const gate = new GuardGate(guard, hands);
		const runtime = await openCodingHarness({
			databasePath,
			session: options.metadata.id,
			workspace: options.metadata.cwd,
			models: modelRuntime,
			model: { provider: model.provider, modelId: model.id },
			thinkingLevel: thinking,
			gate,
			hands,
			refresh: () => gate.refresh(),
			requestAccess: (params, signal) => gate.requestAccess(params, signal),
			promptSections,
		});
		let receipts:
			| Awaited<ReturnType<typeof installOperatorReceipts>>
			| undefined;
		const router = await createSessionRouter(
			{
				harness: runtime.harness,
				modelRuntime,
				beginNavigation: async (conversationId) => {
					const fence = await guard.call("operator_results_select_begin", {
						conversationId,
					});
					if (
						!fence ||
						typeof fence !== "object" ||
						!("token" in fence) ||
						typeof fence.token !== "string"
					)
						throw new Error("Guard did not establish a navigation fence");
					return fence.token;
				},
				endNavigation: async (token, conversationId) => {
					await guard.call("operator_results_select_end", {
						token,
						conversationId,
					});
				},
			},
			BACKGROUND_CONTEXT,
		);
		try {
			receipts = await installOperatorReceipts(
				{
					harness: runtime.harness,
					session: options.metadata.id,
					guard,
					selectedConversationId: () => router.currentConversationId(),
				},
				BACKGROUND_CONTEXT,
			);
		} catch (error) {
			router.close();
			await runtime.harness.close(BACKGROUND_CONTEXT);
			throw new Error("Operator target/receipt recovery unavailable", {
				cause: error,
			});
		}
		debug("harness open");
		return {
			...runtime,
			conversation: router.conversation,
			conversationFacet: router.conversationFacet,
			modelRuntime,
			facetLoader: createStaticFacetLoader([
				defineFacet({
					id: "oq.operator-results.worker",
					setup(env) {
						env.provide(OperatorResults, {
							list(params, context) {
								if (
									!receipts ||
									params.conversationId !== router.currentConversationId()
								)
									throw new Error(
										"Operator receipts unavailable for this conversation",
									);
								return receipts.list(params, context);
							},
						});
					},
				}),
			]),
			async beforeClose() {
				await receipts?.close();
			},
			async cleanup() {
				await receipts?.close();
				router.close();
				hands.close();
				guard.close();
			},
		};
	} catch (error) {
		// Upstream forwards only a message to the server; keep the cause in the
		// private brain log for the operator.
		console.error("oqt brain startup failed:", error);
		hands.close();
		guard.close();
		throw error;
	}
}
