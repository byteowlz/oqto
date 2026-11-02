// Pinned-transport brain: the shared composition (coding-composition.ts) plus
// the retired upstream session-worker wiring (router, receipt service). Copied
// beside the upstream experimental session worker in the private runtime and
// executed only inside the oqt_brain sandbox. Delete with the pinned transport
// (ADR-0071); host2/brain-worker.ts is its replacement.

import { createStaticFacetLoader, defineFacet } from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import { debug, openBrainComposition } from "./coding-composition.ts";
import { installOperatorReceipts } from "./operator-receipts.ts";
import { OperatorResults } from "./operator-results-service.ts";
import type { SessionWorkerRuntime } from "./services/worker.ts";
import { createSessionRouter } from "./session-router.ts";
import type { runSessionWorkerWithHarness } from "./session-worker.ts";

type Options = Parameters<Parameters<typeof runSessionWorkerWithHarness>[1]>[1];

export async function createCodingHarness(
	databasePath: string,
	options: Options,
): Promise<SessionWorkerRuntime> {
	const composition = await openBrainComposition({
		databasePath,
		session: options.metadata.id,
		cwd: options.metadata.cwd,
	});
	const { runtime, modelRuntime, guard } = composition;
	try {
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
		debug("router and receipts ready");
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
				composition.close();
			},
		};
	} catch (error) {
		console.error("oqt brain startup failed:", error);
		composition.close();
		throw error;
	}
}
