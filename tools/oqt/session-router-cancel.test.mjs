// Real SQLite + native providers + attachment input leases. No network/model effects,
// source profiles, JSONL writes, or current-view fallback for recorded-ID cleanup.
import assert from "node:assert/strict";
import { copyFile, mkdtemp, rm, symlink } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import { SessionInputLeases } from "./input-leases.ts";

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
const { Harness, createRegistry } = await load("@earendil-works/pi-durable");
const { createModels, fauxProvider, fauxAssistantMessage } = await load(
	"@earendil-works/pi-ai",
);
const { openNodeSqliteStorage } = await load(
	"@earendil-works/pi-durable/storage/sqlite/node",
);
const AGENT = "pi.agent-controller";
const cancel = (id) => ({
	serviceId: AGENT,
	member: "cancelQueued",
	args: [id],
});
const peer = () => ({ closed: new AbortController().signal });

async function fixture(run, childSelected = false) {
	const dir = await mkdtemp(join(tmpdir(), "oqt-cancel-"));
	let harness,
		host,
		router,
		release = () => {};
	const clients = [];
	try {
		await symlink(join(source, "node_modules"), join(dir, "node_modules"));
		await symlink(
			join(source, "packages/coding-agent/src/experimental/services"),
			join(dir, "services"),
		);
		for (const name of ["session-router.ts", "session-forks.ts"])
			await copyFile(new URL(name, import.meta.url), join(dir, name));
		const { createSessionRouter, fenceConversationTransport } = await import(
			pathToFileURL(join(dir, "session-router.ts"))
		);
		const { SelectedConversationDoc, SessionForks } = await import(
			pathToFileURL(join(dir, "session-forks.ts"))
		);
		const native = (name) =>
			import(
				pathToFileURL(
					join(
						source,
						`packages/coding-agent/src/experimental/services/${name}.ts`,
					),
				)
			);
		const { AgentController } = await native("agent-controller");
		const { Transcript } = await native("transcript");
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
			agent: { model: { provider: "faux", modelId: "faux-1" } },
		});
		const anchor = await root.commit(
			(tx) => tx.appendEntry(root.id, { kind: "fixture.anchor" }),
			context,
		);
		let selected = root;
		if (childSelected) {
			selected = await root.fork(
				anchor.id,
				{ ownership: { kind: "ownerless" } },
				context,
			);
			await harness.commit(async (tx) => {
				(await tx.doc(SelectedConversationDoc)).conversationId = selected.id;
			}, context);
		}
		let started;
		const didStart = new Promise((resolve) => {
			started = resolve;
		});
		const held = new Promise((resolve) => {
			release = resolve;
		});
		faux.setResponses([
			async () => {
				started();
				await held;
				return fauxAssistantMessage("original completed");
			},
		]);
		const running = await root.submit(
			{ type: "input", content: "held original run" },
			context,
		);
		await didStart;
		router = await createSessionRouter({ harness }, context);
		host = await createFacetHost({ facets: [router.conversationFacet] });
		const dispatch = (call, ctx = context) => host.services.invoke(call, ctx);
		const connect = (invoke) => {
			const binding = createRemoteServiceBinding({
				services: [AgentController, Transcript],
				transport: fenceConversationTransport({
					invoke: (call, ctx) => invoke(call, ctx),
					async subscribe(id, mode, listener) {
						return host.services.subscribe(id, mode, listener);
					},
				}),
			});
			clients.push(binding);
			return {
				binding,
				agent: binding.use(AgentController),
				transcript: binding.use(Transcript),
			};
		};
		await run({
			forkServiceId: SessionForks.id,
			dir,
			harness,
			root,
			selected,
			running,
			router,
			dispatch,
			connect,
			open,
			release,
		});
	} finally {
		release();
		for (const binding of clients) await binding.dispose(context);
		if (host) await host.dispose();
		else router?.close();
		if (harness) await harness.close(context);
		await rm(dir, { recursive: true, force: true });
	}
}

test(
	"unstamped native cancellation resolves original submission conversation, never selected pointer",
	{ timeout: 20000 },
	async () => {
		await fixture(
			async ({
				dir,
				harness,
				root,
				selected,
				router,
				dispatch,
				open,
				release,
			}) => {
				assert.notEqual(root.id, selected.id);
				const queued = await root.submit(
					{
						type: "input",
						content: "original queued input",
						whenBusy: "followUp",
					},
					context,
				);
				const untouched = await root.submit(
					{
						type: "input",
						content: "another input stays queued",
						whenBusy: "followUp",
					},
					context,
				);
				assert.equal((await queued.status(context)).status, "queued");
				const before = await selected.context(context);
				// Exactly the native coordinator callback shape: no current-view marker.
				assert.deepEqual(await dispatch(cancel(String(queued.id))), {
					outcome: "cancelled",
				});
				assert.equal((await queued.status(context)).status, "unanswered");
				assert.equal((await queued.status(context)).reason, "aborted");
				assert.equal((await untouched.status(context)).status, "queued");
				assert.equal(router.currentConversationId(), selected.id);
				assert.deepEqual(
					(await selected.context(context)).entries,
					before.entries,
				);
				assert.deepEqual(await dispatch(cancel(String(queued.id))), {
					outcome: "already_consumed",
				});
				assert.deepEqual(await dispatch(cancel("999999999")), {
					outcome: "not_found",
				});
				const passive = await root.submit(
					{ type: "write", entry: { kind: "fixture.passive-receipt" } },
					context,
				);
				assert.deepEqual(await dispatch(cancel(String(passive.id))), {
					outcome: "not_found",
				});
				assert.equal(
					(await passive.status(context)).status,
					"queued",
					"Input cleanup cannot cancel passive context/receipts",
				);
				const foreignHarness = await Harness.open(
					await openNodeSqliteStorage(join(dir, "foreign.sqlite")),
					{ models: createModels(), registry: createRegistry() },
					context,
				);
				try {
					const foreignRoot = await foreignHarness.root(context);
					await foreignRoot.commit(async (tx) => {
						for (let i = 0; i < 40; i++)
							await tx.appendEntry(foreignRoot.id, { kind: "fixture.foreign" });
					}, context);
					const foreignSubmission = await foreignRoot.submit(
						{ type: "input", content: "foreign worker session" },
						context,
					);
					const foreignBefore = await foreignSubmission.wait(context); // no model configured; no inference
					assert.notEqual(foreignSubmission.id, queued.id);
					assert.deepEqual(
						await dispatch(cancel(String(foreignSubmission.id))),
						{ outcome: "not_found" },
					);
					assert.deepEqual(
						await foreignSubmission.status(context),
						foreignBefore,
						"Cancellation is bound to this worker's Harness, never another store",
					);
				} finally {
					await foreignHarness.close(context);
				}
				for (const id of [
					"",
					"0",
					"-1",
					"+1",
					"01",
					"1.0",
					"1e0",
					" 1",
					"9007199254740992",
					`${untouched.id}\n`,
				])
					assert.deepEqual(
						await dispatch(cancel(id)),
						{ outcome: "not_found" },
						id,
					);
				assert.equal(
					(await untouched.status(context)).status,
					"queued",
					"Malformed canonical ID must not alias a valid submission",
				);
				await assert.rejects(
					dispatch({
						serviceId: AGENT,
						member: "prompt",
						args: [{ message: "unstamped input", images: null }],
					}),
					/Stale conversation/,
				);
				await assert.rejects(
					dispatch({
						serviceId: AGENT,
						member: "compact",
						args: [{ customInstructions: null }],
					}),
					/Stale conversation/,
				);
				await assert.rejects(
					dispatch({
						serviceId: "pi.models",
						member: "cycleThinking",
						args: [],
					}),
					/Stale conversation/,
				);
				// Persist cancellation on SQLite, not just an in-memory provider receipt.
				await dispatch(cancel(String(untouched.id)));
				release();
				await harness.waitForIdle(context);
				await harness.close(context);
				const recovered = await open();
				try {
					assert.equal(
						(
							await (
								await recovered.submission(queued.id, context)
							).status(context)
						).reason,
						"aborted",
					);
					assert.equal(
						(await recovered.conversation(selected.id, context)).id,
						selected.id,
					);
				} finally {
					await recovered.close(context);
				}
			},
			true,
		);
	},
);

test(
	"real input leases revoke/detach cancel only attachment-owned native IDs through unstamped worker calls",
	{ timeout: 20000 },
	async () => {
		await fixture(
			async ({ harness, root, dispatch, connect, forkServiceId }) => {
				const cancelled = [],
					stopped = [];
				const leases = new SessionInputLeases(() => {});
				const decider = peer();
				const grant = async (attachment, remote) => {
					const request = leases.request(
						remote,
						attachment,
						"remote fixture",
						new AbortController().signal,
					);
					await leases.decide(decider, leases.view().pending.id, true);
					assert.equal((await request).granted, true);
				};
				const register = (attachment) =>
					leases.register(
						attachment,
						async (id) => {
							// This is the exact private coordinator dispatch used during cleanup.
							const result = await dispatch(cancel(id));
							cancelled.push({ attachment, id, result });
						},
						async () => {
							stopped.push(attachment);
						},
					);
				const remoteClient = (attachment) =>
					connect(async (call, ctx) => {
						const admission = await leases.before(attachment, call);
						leases.assertCurrent(admission);
						const result = await dispatch(call, ctx);
						await leases.after(admission, call, result);
						return result;
					});
				try {
					register("remote-a");
					const ownerA = peer();
					await grant("remote-a", ownerA);
					for (const member of ["select", "fork"])
						await assert.rejects(
							leases.before("remote-a", {
								serviceId: forkServiceId,
								member,
								args: [],
							}),
							/unsupported/,
						);
					const clientA = remoteClient("remote-a");
					await clientA.binding.ready(context);
					const ownedA = await clientA.agent.steer(
						{ message: "attachment A queued", images: null },
						context,
					);
					assert.equal(ownedA.accepted, true);
					const directOwned = await clientA.agent.steer(
						{ message: "explicit owned cancellation", images: null },
						context,
					);
					assert.deepEqual(
						await clientA.agent.cancelQueued(directOwned.entryId, context),
						{ outcome: "cancelled" },
					);
					assert.equal(
						(
							await (
								await harness.submission(Number(directOwned.entryId), context)
							).status(context)
						).reason,
						"aborted",
					);
					const foreign = await root.submit(
						{
							type: "input",
							content: "not owned by remote attachment",
							whenBusy: "followUp",
						},
						context,
					);
					await assert.rejects(
						clientA.agent.cancelQueued(String(foreign.id), context),
						/belongs to another source/,
					);
					await assert.rejects(
						clientA.agent.cancelQueued("999999999", context),
						/belongs to another source/,
					);
					assert.equal((await foreign.status(context)).status, "queued");
					leases.release(ownerA);
					await leases.unregister("remote-a"); // drains scheduled revoke cleanup
					assert.deepEqual(cancelled[0], {
						attachment: "remote-a",
						id: ownedA.entryId,
						result: { outcome: "cancelled" },
					});
					assert.equal(
						(
							await (
								await harness.submission(Number(ownedA.entryId), context)
							).status(context)
						).reason,
						"aborted",
					);
					assert.equal((await foreign.status(context)).status, "queued");
					await assert.rejects(
						clientA.agent.prompt(
							{ message: "revoked input", images: null },
							context,
						),
						/unavailable|lease required/,
					);

					register("remote-b");
					const ownerB = peer();
					await grant("remote-b", ownerB);
					const clientB = remoteClient("remote-b");
					await clientB.binding.ready(context);
					const ownedB = await clientB.agent.steer(
						{ message: "attachment B queued", images: null },
						context,
					);
					assert.equal(ownedB.accepted, true);
					// Native detachment, not a UI cancellation request, invokes the ID callback.
					await leases.unregister("remote-b");
					assert.deepEqual(cancelled[1], {
						attachment: "remote-b",
						id: ownedB.entryId,
						result: { outcome: "cancelled" },
					});
					assert.equal(
						(
							await (
								await harness.submission(Number(ownedB.entryId), context)
							).status(context)
						).reason,
						"aborted",
					);
					assert.equal((await foreign.status(context)).status, "queued");
					assert.deepEqual(
						stopped,
						[],
						"Successful ID cleanup must not retire the worker as an uncertain fallback",
					);
					await dispatch(cancel(String(foreign.id)));
				} finally {
					await leases.close();
				}
			},
		);
	},
);
