/** Session worker: the one process that owns a Session's Durable storage.
 * It connects back to the server's private worker socket with a single-use,
 * per-generation token and serves the worker contracts as `session:<id>`.
 * Only the server's own principal may use them: clients reach a Session through
 * the server facade, where the guard admits input first.
 */
import {
	type Ctx,
	createHost,
	forbidden,
	type ImplementationOf,
	webSocketLink,
} from "@earendil-works/chord";
import {
	BACKGROUND_CONTEXT,
	withAbortSignal,
} from "@earendil-works/chord/context";
import {
	clampThinkingLevel,
	getSupportedThinkingLevels,
	type ModelThinkingLevel,
} from "@earendil-works/pi-ai";
import {
	AgentDoc,
	type AgentState,
	ConversationBusy,
	type ConversationId,
	type Conversation as DurableConversation,
	type EntryId,
	type Harness,
	type SubmissionId,
} from "@earendil-works/pi-durable";
import { Type } from "typebox";
import { WebSocket } from "ws";
import { createSessionForks } from "../experimental/session-forks.ts";
import {
	ConversationSummary,
	ModelsState,
	type Principal,
	principalKey,
	SESSION_ID,
	WorkerConversation,
	WorkerSession,
} from "./contract.ts";

export interface WorkerInit {
	readonly session: string;
	readonly generation: number;
	readonly database: string;
	/** Execution directory of the Session's hands (coding workers). */
	readonly cwd?: string;
	/** The server's private worker socket and this generation's token. */
	readonly socket: string;
	readonly token: string;
}

/** The model catalog a worker can offer; the brain's ModelRuntime fits. */
export interface ModelCatalog {
	getAvailableSnapshot(): readonly {
		provider: string;
		id: string;
		name: string;
		reasoning: boolean;
	}[];
	getModel(
		provider: string,
		id: string,
	):
		| { provider: string; id: string; name: string; reasoning: boolean }
		| undefined;
	refresh?(options: {
		signal?: AbortSignal;
	}): Promise<{ errors: ReadonlyMap<string, Error> }>;
}

/** Guard-owned navigation fence (operator receipts are conversation-bound). */
export interface NavigationFence {
	begin(conversation: number): Promise<string>;
	end(token: string, conversation: number): Promise<void>;
}

export interface OpenedSession {
	readonly harness: Harness;
	readonly root: DurableConversation;
	readonly models?: ModelCatalog;
	readonly navigation?: NavigationFence;
	close(): Promise<void>;
}

/** Composition seam: data comes from the server; code is chosen by the entry module. */
export type OpenSession = (init: WorkerInit) => Promise<OpenedSession>;

export const INIT_ENV = "OQT_HOST2_INIT";

export function readInit(value = process.env[INIT_ENV]): WorkerInit {
	const init = JSON.parse(value ?? "null") as WorkerInit | null;
	if (
		!init ||
		!SESSION_ID.test(init.session) ||
		!Number.isSafeInteger(init.generation) ||
		typeof init.database !== "string" ||
		typeof init.socket !== "string" ||
		!/^[0-9a-f]{64}$/.test(init.token)
	)
		throw new Error("Invalid worker init");
	return init;
}

/** A Durable Context for a chord call: the caller's cancellation, no ambient values. */
const durable = (ctx: Ctx<Principal>) =>
	withAbortSignal(ctx.signal, BACKGROUND_CONTEXT);
const message = (error: unknown) =>
	(error instanceof Error ? error.message : String(error)).slice(0, 2048);
const refused = (error: unknown) => ({
	accepted: false as const,
	error: message(error),
});
const submissionId = (value: string) => {
	const id = Number(value);
	return /^[1-9][0-9]*$/.test(value) && Number.isSafeInteger(id)
		? (id as SubmissionId)
		: undefined;
};

interface Summary {
	id: number;
	parent?: { conversation: number; at: string };
	entries: number;
}

export async function runWorker(
	open: OpenSession,
	init = readInit(),
): Promise<void> {
	// The token is consumed; nothing this process spawns inherits it.
	delete process.env[INIT_ENV];
	const opened = await open(init);
	const { harness, root, models: catalog, navigation } = opened;
	harness.resume();
	let catalogRevision = 0;
	const readCatalog = (errors?: Record<string, string>) => ({
		revision: ++catalogRevision,
		available: (catalog?.getAvailableSnapshot() ?? []).map((model) => ({
			provider: model.provider,
			modelId: model.id,
			name: model.name,
			reasoning: model.reasoning,
		})),
		refresh:
			errors && Object.keys(errors).length
				? { status: "warning" as const, errors }
				: { status: "idle" as const },
	});
	const conversationOf = async (id: number) => {
		const found = await harness.conversation(
			id as ConversationId,
			BACKGROUND_CONTEXT,
		);
		if (!found) throw forbidden();
		return found;
	};
	/** Applies a selection change: the durable document is the authority. */
	const fenced = async <T>(conversation: number, change: () => Promise<T>) => {
		const token = navigation ? await navigation.begin(conversation) : undefined;
		try {
			return await change();
		} finally {
			if (token !== undefined) await navigation?.end(token, conversation);
		}
	};
	const forks = createSessionForks(harness, async () => {});
	const listConversations = async (): Promise<Summary[]> => {
		const page = await harness.commit(
			(tx) => tx.scanConversations({}, 100),
			BACKGROUND_CONTEXT,
		);
		const out: Summary[] = [];
		for (const record of page.items) {
			const entries = await harness.commit(
				(tx) => tx.scanEntries({ conversationId: record.id }, 1000),
				BACKGROUND_CONTEXT,
			);
			out.push({
				id: Number(record.id),
				...(record.parent
					? {
							parent: {
								conversation: Number(record.parent.conversationId),
								at: String(record.parent.at),
							},
						}
					: {}),
				entries: entries.items.length,
			});
		}
		return out;
	};
	const socket = new WebSocket(`ws+unix://${init.socket}:/`, {
		headers: { authorization: `Bearer ${init.token}` },
		maxPayload: 16 * 1024 * 1024,
	});
	// Attach the link before the socket opens (see client.ts).
	const host = createHost<Principal>({
		name: `session:${init.session}`,
		self: { kind: "host" },
		principalKey,
		onError: (error) => console.error(`[oqt session:${init.session}]`, error),
		upstream: {
			link: webSocketLink(socket as never),
			principal: { kind: "server" },
		},
	});
	const onlyServer = (principal: Principal, session: string) => {
		if (session !== init.session || principal.kind !== "server")
			throw forbidden();
	};
	host.serve(WorkerSession, ({ session }, principal, self) => {
		onlyServer(principal, session);
		const selected = self.state("selected", Type.Integer({ minimum: 1 }), {
			init: async () => Number(await forks.selected(BACKGROUND_CONTEXT)),
		});
		const conversations = self.state(
			"conversations",
			Type.Array(ConversationSummary),
			{ init: () => listConversations() },
		);
		const models = self.state("models", ModelsState, readCatalog());
		const publish = async (ctx: Ctx<Principal>, id: number) => {
			selected.replace(ctx, id);
			conversations.replace(ctx, await listConversations());
		};
		return {
			variant: "default",
			root: self.state("root", Type.Integer({ minimum: 1 }), Number(root.id)),
			selected,
			conversations,
			models,
			select: async ({ conversation }, ctx) => {
				try {
					await fenced(conversation, () =>
						forks.select(conversation as ConversationId, durable(ctx)),
					);
					await publish(ctx, conversation);
					return { accepted: true, id: String(conversation) };
				} catch (error) {
					return refused(error);
				}
			},
			refreshModels: async (ctx) => {
				models.change(ctx, (draft) => {
					draft.refresh = { status: "refreshing" };
				});
				const result = await catalog?.refresh?.({ signal: ctx.signal });
				const errors = Object.fromEntries(
					[...(result?.errors ?? new Map())].map(([id, error]) => [
						id,
						error.message,
					]),
				);
				models.replace(ctx, readCatalog(errors));
			},
			fork: async ({ conversation, at }, ctx) => {
				const entry = Number(at);
				if (!Number.isSafeInteger(entry) || entry < 1)
					return refused("Invalid entry");
				try {
					const child = await fenced(conversation, () =>
						forks.fork(
							conversation as ConversationId,
							entry as EntryId,
							durable(ctx),
						),
					);
					await publish(ctx, Number(child));
					return { accepted: true, id: String(child) };
				} catch (error) {
					return refused(error);
				}
			},
		};
	});
	host.serve(
		WorkerConversation,
		({ session, conversation }, principal, self) => {
			onlyServer(principal, session);
			const handle = self.resource("conversation", {
				version: 1,
				open: () => conversationOf(conversation),
			});
			// Durable's conversation watch: the current view, then each change (commits
			// made while the listener runs arrive folded, which suits slow clients).
			const attached = self.resource("view", {
				version: 2,
				deps: [handle],
				open: async () => handle.value.watch(BACKGROUND_CONTEXT),
				close: async (watch) => {
					await watch.stop();
				},
			});
			const view = self.state("view", Type.Unknown(), {
				init: () => attached.value.value as never,
			});
			// chord listens to subscribables; adapt the watch handle (its sole listener).
			self.on(
				"view",
				attached,
				(watch: Awaited<ReturnType<DurableConversation["watch"]>>) => ({
					subscribe(listener: (value: unknown) => void) {
						watch.start(async (value) => listener(value));
						return () => void watch.stop();
					},
				}),
				(value, ctx) => view.replace(ctx, value as never),
			);
			const agent = async (ctx: Ctx<Principal>) =>
				((await harness.snapshot(AgentDoc, handle.value.id, durable(ctx))) ??
					{}) as AgentState;
			const levels = async (
				ctx: Ctx<Principal>,
			): Promise<ModelThinkingLevel[]> => {
				const ref = (await agent(ctx)).model;
				const model = ref && catalog?.getModel(ref.provider, ref.modelId);
				return model ? getSupportedThinkingLevels(model as never) : ["off"];
			};
			const submit = async (
				whenBusy: "reject" | "steer" | "followUp",
				{ text, requestId }: { text: string; requestId?: string },
				ctx: Ctx<Principal>,
			) => {
				try {
					const submission = await handle.value.submit(
						{
							type: "input",
							content: text,
							whenBusy,
							...(requestId ? { requestId } : {}),
						},
						durable(ctx),
					);
					return { accepted: true as const, id: String(submission.id) };
				} catch (error) {
					return refused(
						error instanceof ConversationBusy
							? "The conversation is busy"
							: error,
					);
				}
			};
			const implementation: ImplementationOf<
				typeof WorkerConversation,
				Principal
			> = {
				variant: "default",
				view,
				prompt: (input, ctx) => submit("reject", input, ctx),
				steer: (input, ctx) => submit("steer", input, ctx),
				followUp: (input, ctx) => submit("followUp", input, ctx),
				cancelQueued: async ({ id }, ctx) => {
					const parsed = submissionId(id);
					if (parsed === undefined) return { outcome: "not_found" as const };
					const result = await harness.abortSubmission(
						parsed,
						durable(ctx),
						handle.value.id,
					);
					return {
						outcome:
							result === "aborted"
								? ("cancelled" as const)
								: result === "not_found"
									? ("not_found" as const)
									: ("already_consumed" as const),
					};
				},
				abort: async (ctx) => {
					await handle.value.abort(durable(ctx));
				},
				compact: async ({ instructions }, ctx) => {
					try {
						return {
							accepted: true as const,
							id: String(
								await handle.value.compact(instructions, durable(ctx)),
							),
						};
					} catch (error) {
						return refused(error);
					}
				},
				selectModel: async ({ provider, modelId }, ctx) => {
					const model = catalog?.getModel(provider, modelId);
					if (!model) throw new Error(`Unknown model: ${provider}/${modelId}`);
					const thinkingLevel = clampThinkingLevel(
						model as never,
						(await agent(ctx)).thinkingLevel ?? "off",
					);
					await handle.value.configure(
						{
							model: { provider: model.provider, modelId: model.id },
							thinkingLevel,
						},
						durable(ctx),
					);
				},
				selectThinking: async ({ level }, ctx) => {
					const available = await levels(ctx);
					if (!available.includes(level as ModelThinkingLevel))
						throw new Error(
							`Thinking level ${level} is unavailable; choose one of: ${available.join(", ")}`,
						);
					await handle.value.configure(
						{ thinkingLevel: level as ModelThinkingLevel },
						durable(ctx),
					);
				},
				cycleThinking: async (ctx) => {
					const available = await levels(ctx);
					const current = (await agent(ctx)).thinkingLevel ?? "off";
					const next =
						available[(available.indexOf(current) + 1) % available.length] ??
						"off";
					await handle.value.configure({ thinkingLevel: next }, durable(ctx));
				},
				thinkingLevels: (ctx) => levels(ctx),
			};
			return implementation;
		},
	);
	let closing: Promise<void> | undefined;
	const shutdown = () => {
		closing ??= (async () => {
			await host.close().catch(() => {});
			await opened.close().catch(() => {});
			process.exit(0);
		})();
		return closing;
	};
	// Without its server this worker has no clients and no authority to keep running.
	socket.once("close", () => void shutdown());
	process.once("SIGTERM", () => void shutdown());
	await new Promise<void>((resolve, reject) => {
		socket.once("open", () => resolve());
		socket.once("error", reject);
	});
}
