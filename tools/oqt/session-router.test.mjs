// Native SQLite + actual Chord services/providers; never real sessions or model network.
import assert from "node:assert/strict";
import { copyFile, mkdtemp, rm, symlink } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

const source = process.env.OQT_PI_SOURCE;
if (!source) throw new Error("OQT_PI_SOURCE must name pinned dependencies");
const require = createRequire(join(source, "package.json"));
const load = (name) => import(pathToFileURL(require.resolve(name)));
const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);
const { createFacetHost, createRemoteServiceBinding } = await load(
	"@earendil-works/chord",
);
const { Harness, createRegistry, AgentDoc } = await load(
	"@earendil-works/pi-durable",
);
const { createModels, fauxProvider, fauxAssistantMessage } = await load(
	"@earendil-works/pi-ai",
);
const { openNodeSqliteStorage } = await load(
	"@earendil-works/pi-durable/storage/sqlite/node",
);
const tick = () => new Promise((resolve) => setTimeout(resolve, 5));
async function until(check) {
	for (let i = 0; i < 200; i++) {
		if (check()) return;
		await tick();
	}
	assert.fail("Timed out waiting for native service state");
}

test(
	"worker router coherently rebinds native providers, fences stale clients, rejects busy navigation and reopens selected SQLite conversation",
	{ timeout: 20000 },
	async () => {
		const dir = await mkdtemp(join(tmpdir(), "oqt-router-"));
		let harness, host, router;
		const clients = [];
		try {
			await symlink(join(source, "node_modules"), join(dir, "node_modules"));
			await symlink(
				join(source, "packages/coding-agent/src/experimental/services"),
				join(dir, "services"),
			);
			for (const name of ["session-router.ts", "session-forks.ts"])
				await copyFile(new URL(name, import.meta.url), join(dir, name));
			const {
				createSessionRouter,
				fenceConversationTransport,
				invokeAtConversation,
				captureConversationContext,
			} = await import(pathToFileURL(join(dir, "session-router.ts")));
			const { SessionForks } = await import(
				pathToFileURL(join(dir, "session-forks.ts"))
			);
			const service = async (name) =>
				import(
					pathToFileURL(
						join(
							source,
							`packages/coding-agent/src/experimental/services/${name}.ts`,
						),
					)
				);
			const { Transcript } = await service("transcript");
			const { Models } = await service("models");
			const { AgentController } = await service("agent-controller");
			const models = createModels(),
				faux = fauxProvider();
			models.setProvider(faux.provider);
			const open = async () =>
				Harness.open(
					await openNodeSqliteStorage(join(dir, "session.sqlite")),
					{ models, registry: createRegistry() },
					context,
				);
			harness = await open();
			const root = await harness.root(context, {
				agent: {
					model: { provider: "faux", modelId: "faux-1" },
					thinkingLevel: "high",
				},
			});
			const first = await root.commit(
				(tx) =>
					tx.appendEntry(root.id, {
						kind: "fixture.same",
						data: { text: "same" },
					}),
				context,
			);
			const second = await root.commit(
				(tx) =>
					tx.appendEntry(root.id, {
						kind: "fixture.same",
						data: { text: "same" },
					}),
				context,
			);
			const selected = [];
			router = await createSessionRouter(
				{
					harness,
					beginNavigation: async (id) => {
						assert.equal(router.currentConversationId(), id);
						return { source: id };
					},
					endNavigation: async (fence, id) => {
						assert.equal(fence.source, router.currentConversationId());
						selected.push(id);
					},
				},
				context,
			);
			host = await createFacetHost({ facets: [router.conversationFacet] });
			const connect = () => {
				let paused = false;
				const pending = [];
				const native = {
					invoke: (call, ctx) => host.services.invoke(call, ctx),
					async subscribe(id, mode, listener) {
						return host.services.subscribe(id, mode, (frame, ctx) => {
							if (paused) pending.push(() => listener(frame, ctx));
							else listener(frame, ctx);
						});
					},
				};
				const binding = createRemoteServiceBinding({
					services: [Transcript, Models, AgentController, SessionForks],
					transport: fenceConversationTransport(native),
				});
				const client = {
					binding,
					transcript: binding.use(Transcript),
					models: binding.use(Models),
					agent: binding.use(AgentController),
					forks: binding.use(SessionForks),
					pause() {
						paused = true;
					},
					resume() {
						paused = false;
						for (const deliver of pending.splice(0)) deliver();
					},
				};
				clients.push(client);
				return client;
			};
			const one = connect(),
				two = connect();
			await Promise.all(clients.map((client) => client.binding.ready(context)));
			const transcriptIdentity = one.transcript.state,
				modelsIdentity = one.models.state;
			assert.equal(one.transcript.state.value.conversation.id, root.id);
			const openedModelPicker = one.models.state.value;
			const openedInputView = one.transcript.state.value;
			const pluginPickerContext = captureConversationContext(
				openedInputView,
				context,
			);
			two.pause();
			const child = await one.forks.fork(root.id, first.id, context);
			await until(
				() =>
					one.transcript.state.value?.conversation.id === child &&
					one.models.state.value?.configuration.thinkingLevel === "high",
			);
			assert.equal(router.currentConversationId(), child);
			await assert.rejects(
				one.models.selectThinking("off", pluginPickerContext),
				/Stale conversation/,
			);
			await assert.rejects(
				one.models.select(
					{ provider: "faux", modelId: "faux-1" },
					pluginPickerContext,
				),
				/Stale conversation/,
			);
			await assert.rejects(
				invokeAtConversation(
					one.models.selectThinking,
					["off"],
					openedModelPicker,
					context,
				),
				/Stale conversation/,
			);
			await assert.rejects(
				invokeAtConversation(
					one.models.select,
					[{ provider: "faux", modelId: "faux-1" }],
					openedModelPicker,
					context,
				),
				/Stale conversation/,
			);
			await assert.rejects(
				invokeAtConversation(
					one.agent.compact,
					[{ customInstructions: null }],
					openedInputView,
					context,
				),
				/Stale conversation/,
			);
			await assert.rejects(
				invokeAtConversation(
					one.agent.prompt,
					[{ message: "late input", images: null }],
					openedInputView,
					context,
				),
				/Stale conversation/,
			);
			assert.equal(one.transcript.state, transcriptIdentity);
			assert.equal(one.models.state, modelsIdentity);
			assert.deepEqual(
				one.transcript.state.value.entries.map((entry) => entry.id),
				[first.id],
			);
			await assert.rejects(
				two.agent.prompt({ message: "stale input", images: null }, context),
				/Stale conversation/,
			);
			await assert.rejects(
				two.models.cycleThinking(context),
				/Stale conversation/,
			);
			await assert.rejects(
				two.forks.select(root.id, context),
				/Stale conversation/,
			);
			two.resume();
			await until(() => two.transcript.state.value?.conversation.id === child);
			await one.models.cycleThinking(context);
			await until(
				() => one.models.state.value.configuration.thinkingLevel === "off",
			);
			assert.equal(
				(await harness.snapshot(AgentDoc, root.id, context)).thinkingLevel,
				"high",
			);
			assert.equal(
				(await harness.snapshot(AgentDoc, child, context)).thinkingLevel,
				"off",
			);
			await root.commit(
				(tx) => tx.appendEntry(root.id, { kind: "fixture.old-observer" }),
				context,
			);
			await tick();
			assert.equal(one.transcript.state.value.conversation.id, child);
			assert.equal(
				one.transcript.state.value.entries.some(
					(entry) => entry.id === second.id,
				),
				false,
			);
			let release;
			const held = new Promise((resolve) => {
				release = resolve;
			});
			faux.setResponses([
				async () => {
					await held;
					return fauxAssistantMessage("child answer");
				},
			]);
			const operation = await one.agent.prompt(
				{ message: "child-only prompt", images: null },
				context,
			);
			assert.equal(operation.accepted, true);
			await assert.rejects(
				one.forks.select(root.id, context),
				/native tasks|queued submissions/,
			);
			release();
			await harness.waitForIdle(context);
			await until(() =>
				one.transcript.state.value.entries.some(
					(entry) => entry.kind === "pi.user",
				),
			);
			assert.equal(
				JSON.stringify((await root.context(context)).messages).includes(
					"child-only prompt",
				),
				false,
			);
			const childHandle = await harness.conversation(child, context);
			assert.equal(
				JSON.stringify((await childHandle.context(context)).messages).includes(
					"child-only prompt",
				),
				true,
			);
			const competing = await Promise.allSettled([
				one.forks.select(root.id, context),
				two.forks.select(child, context),
			]);
			assert.equal(competing[0].status, "fulfilled");
			assert.equal(competing[1].status, "rejected");
			assert.match(competing[1].reason.message, /Stale conversation/);
			await until(() => one.transcript.state.value.conversation.id === root.id);
			await one.forks.select(child, context);
			await until(() => one.transcript.state.value.conversation.id === child);
			assert.deepEqual(selected, [child, root.id, child]);
			for (const client of clients.splice(0))
				await client.binding.dispose(context);
			await host.dispose();
			host = undefined;
			await harness.close(context);
			harness = undefined;
			harness = await open();
			router = await createSessionRouter({ harness }, context);
			assert.equal(router.conversation.id, child);
			host = await createFacetHost({ facets: [router.conversationFacet] });
			const reopened = connect();
			await reopened.binding.ready(context);
			assert.equal(reopened.transcript.state.value.conversation.id, child);
			assert.equal(
				reopened.models.state.value.configuration.thinkingLevel,
				"off",
			);
			for (const client of clients.splice(0))
				await client.binding.dispose(context);
			await host.dispose();
			host = undefined;
			const completions = [];
			router = await createSessionRouter(
				{
					harness,
					beginNavigation: async (id) => ({ source: id }),
					endNavigation: async (_fence, id) => {
						completions.push(id);
						throw new Error("injected guard/rebind failure");
					},
				},
				context,
			);
			host = await createFacetHost({ facets: [router.conversationFacet] });
			const failedClient = connect();
			await failedClient.binding.ready(context);
			await assert.rejects(
				failedClient.forks.select(root.id, context),
				/injected guard\/rebind failure/,
			);
			await assert.rejects(
				failedClient.models.cycleThinking(context),
				/router unavailable/,
			);
			await assert.rejects(
				failedClient.agent.prompt(
					{ message: "must freeze", images: null },
					context,
				),
				/router unavailable/,
			);
			await assert.rejects(
				failedClient.forks.select(child, context),
				/router unavailable/,
			);
			assert.deepEqual(
				await failedClient.agent.cancelQueued(operation.operationId, context),
				{ outcome: "already_consumed" },
				"A frozen rebind must still allow exact-ID cleanup of the original conversation",
			);
			assert.deepEqual(
				completions,
				[root.id],
				"Postcommit failure must not unlock/restore guard to previous branch",
			);
			for (const client of clients.splice(0))
				await client.binding.dispose(context);
			await host.dispose();
			host = undefined;
			await harness.close(context);
			harness = await open();
			router = await createSessionRouter({ harness }, context);
			assert.equal(
				router.conversation.id,
				root.id,
				"Recovery uses committed target, not the old active pointer",
			);
		} finally {
			for (const client of clients) await client.binding.dispose(context);
			if (host) await host.dispose();
			else router?.close();
			if (harness) await harness.close(context);
			await rm(dir, { recursive: true, force: true });
		}
	},
);
