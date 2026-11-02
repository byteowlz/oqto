/** Test-only worker composition: a scripted faux model that answers per prompt
 * ("slow" runs a ~1 s tool, anything else is echoed) and one slow tool, so runs
 * are observably in progress. Never used by the oqt launcher. */
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import { type AssistantMessage, Type } from "@earendil-works/pi-ai";
import { createModels } from "@earendil-works/pi-ai/models";
import {
	fauxAssistantMessage,
	fauxProvider,
	fauxText,
	fauxToolCall,
} from "@earendil-works/pi-ai/providers/faux";
import {
	createRegistry,
	defineExtension,
	defineTool,
	Harness,
} from "@earendil-works/pi-durable";
import { openNodeSqliteStorage } from "@earendil-works/pi-durable/storage/sqlite/node";
import { runWorker } from "./worker.ts";

const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
type Message = { role: string; content: unknown };
const textOf = (message: Message | undefined) =>
	typeof message?.content === "string"
		? message.content
		: Array.isArray(message?.content)
			? (message.content as { type: string; text?: string }[])
					.map((part) => part.text ?? "")
					.join("")
			: "";

let calls = 0;
const respond = (context: {
	messages: readonly Message[];
}): AssistantMessage => {
	const last = context.messages.at(-1);
	if (last?.role === "toolResult")
		return fauxAssistantMessage([fauxText("slow-tool-finished")]);
	const prompt = textOf(
		[...context.messages].reverse().find((message) => message.role === "user"),
	);
	if (prompt.includes("slow"))
		return fauxAssistantMessage(
			[fauxToolCall("slow", {}, { id: `call-${++calls}` })],
			{ stopReason: "toolUse" },
		);
	return fauxAssistantMessage([fauxText(`reply:${prompt}`)]);
};

await runWorker(async (init) => {
	const faux = fauxProvider({
		tokensPerSecond: 200,
		tokenSize: { min: 4, max: 4 },
	});
	faux.setResponses(Array.from({ length: 500 }, () => respond as never));
	const models = createModels();
	models.setProvider(faux.provider);
	const registry = createRegistry();
	registry.install(
		defineExtension({
			name: "test",
			tools: [
				defineTool({
					name: "slow",
					description: "Takes about a second",
					parameters: Type.Object({}),
					execute: async (_args, api) => {
						for (let n = 1; n <= 5; n++) {
							api.output(`${n}\n`);
							await pause(200);
						}
						return {};
					},
				}),
			],
		}),
	);
	const harness = await Harness.open(
		await openNodeSqliteStorage(init.database),
		{ models, registry },
		BACKGROUND_CONTEXT,
	);
	const root = await harness.root(BACKGROUND_CONTEXT, {
		agent: { model: { provider: "faux", modelId: "faux-1" } },
	});
	return {
		harness,
		root,
		models: {
			getAvailableSnapshot: () => models.getModels("faux"),
			getModel: (provider: string, id: string) => models.getModel(provider, id),
		},
		close: () => harness.close(BACKGROUND_CONTEXT),
	};
});
