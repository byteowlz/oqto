import type { ChatEngineHandle, ChatTurnUpdate } from "./chat-contract";
import {
	type FlashChatOptions,
	createFlashSessionChat,
} from "./flash-session-chat";
export { flashHistory, flashIdentity } from "./flash-session-chat";
export type { FlashChatOptions } from "./flash-session-chat";

type SessionChat = ReturnType<typeof createFlashSessionChat>;
type Descriptor = Awaited<ReturnType<SessionChat["sessions"]>>[number];
type Observer = (update: ChatTurnUpdate) => void;
type Context = {
	source: SessionChat;
	opening: Promise<void>;
	ready: boolean;
	users: number;
	bindings: number;
	used: number;
	unwatch: () => void;
};
const MAX_CONTEXTS = 8;

/** One retained owner anchor, immutable target peers, bounded disposable projections. */
export function createFlashChat(options: FlashChatOptions) {
	const root = createFlashSessionChat(options);
	const contexts = new Map<string, Context>();
	const known = new Map<
		string,
		Pick<Descriptor, "publicSessionId" | "workDirectoryId">
	>();
	const listeners = new Set<() => void>();
	const observers = new Map<string, Set<Observer>>();
	let opening: Promise<void> | undefined;
	let admission: Promise<unknown> = Promise.resolve();
	let clock = 0;
	let disposed = false;
	const assertOpen = () => {
		if (disposed) throw new Error("Flash chat disposed");
	};
	const notify = () => {
		if (!disposed) for (const listener of listeners) listener();
	};
	const install = (id: string, source: SessionChat): Context => {
		const context: Context = {
			source,
			opening: Promise.resolve(),
			ready: false,
			users: 0,
			bindings: 0,
			used: ++clock,
			unwatch: () => {},
		};
		context.unwatch = source.subscribe(notify);
		contexts.set(id, context);
		context.opening = source.open().then(() => {
			if (source.getIdentity().publicSessionId !== id)
				throw new Error("Native peer returned a different Session identity");
			context.ready = true;
		});
		return context;
	};
	const open = (): Promise<void> => {
		assertOpen();
		opening ??= root
			.open()
			.then(async () => {
				assertOpen();
				const identity = root.getIdentity();
				known.set(identity.publicSessionId, identity);
				await install(identity.publicSessionId, root).opening;
			})
			.catch((error) => {
				opening = undefined;
				throw error;
			});
		return opening;
	};
	const sessions = async () => {
		await open();
		const catalog = await root.sessions();
		assertOpen();
		const directory = root.getIdentity().workDirectoryId;
		const ids = new Set<string>();
		for (const descriptor of catalog) {
			if (
				descriptor.workDirectoryId !== directory ||
				ids.has(descriptor.publicSessionId)
			)
				throw new Error("Invalid native browser Session catalog");
			ids.add(descriptor.publicSessionId);
			known.set(descriptor.publicSessionId, descriptor);
		}
		return catalog;
	};
	const check = (id: string) => {
		assertOpen();
		if (!known.has(id)) throw new Error(`Unknown Flash adapter session: ${id}`);
	};
	const acquire = async (id: string): Promise<Context> => {
		await open();
		if (!known.has(id)) await sessions();
		check(id);
		const result = admission.then(async () => {
			assertOpen();
			let context = contexts.get(id);
			if (!context) {
				if (contexts.size >= MAX_CONTEXTS) {
					const candidate = [...contexts.entries()]
						.filter(
							([, item]) =>
								item.source !== root && item.users === 0 && item.bindings === 0,
						)
						.sort((a, b) => a[1].used - b[1].used)[0];
					if (!candidate)
						throw new Error(
							"Too many simultaneously observed browser Sessions; close a chat pane first",
						);
					contexts.delete(candidate[0]);
					candidate[1].unwatch();
					await candidate[1].source.dispose();
				}
				assertOpen();
				context = install(id, createFlashSessionChat(options, id));
			}
			context.used = ++clock;
			context.users++;
			try {
				await context.opening;
				return context;
			} catch (error) {
				context.users--;
				if (context.source !== root) {
					contexts.delete(id);
					context.unwatch();
					await context.source.dispose();
				}
				throw error;
			}
		});
		// A rejected admission must not poison subsequent independent requests.
		admission = result.then(
			() => undefined,
			() => undefined,
		);
		return result;
	};
	const use = async <T>(
		id: string,
		run: (context: Context) => T | Promise<T>,
	): Promise<T> => {
		const context = await acquire(id);
		try {
			assertOpen();
			return await run(context);
		} finally {
			context.users--;
		}
	};
	const report = (id: string, error: unknown) => {
		if (disposed) return;
		for (const observer of observers.get(id) ?? []) {
			observer({
				kind: "draft",
				draft: {
					parts: [
						{
							type: "error",
							id: `flash:error:${crypto.randomUUID()}`,
							text: error instanceof Error ? error.message : String(error),
							retrying: false,
						},
					],
				},
			});
			observer({ kind: "ended", reason: "error" });
		}
	};
	const chat: ChatEngineHandle = {
		bind(id, listener) {
			check(id);
			let stopped = false;
			let detach: (() => void) | undefined;
			const set = observers.get(id) ?? new Set<Observer>();
			observers.set(id, set);
			set.add(listener);
			const attach = (context: Context) => {
				if (stopped || disposed) return;
				context.bindings++;
				const stop = context.source.chat.bind(id, listener);
				detach = () => {
					stop();
					context.bindings--;
				};
			};
			const cached = contexts.get(id);
			if (cached?.ready) attach(cached);
			else
				void use(id, attach).catch((error) => {
					if (!stopped) report(id, error);
				});
			return () => {
				stopped = true;
				set.delete(listener);
				if (!set.size) observers.delete(id);
				detach?.();
			};
		},
		send(id, text, mode) {
			check(id);
			const requestId = crypto.randomUUID();
			void use(id, ({ source }) =>
				source.sendRequest(id, text, mode, requestId),
			).catch((error) => report(id, error));
			return requestId;
		},
		abort(id) {
			check(id);
			void use(id, ({ source }) => source.chat.abort(id)).catch((error) =>
				report(id, error),
			);
		},
	};
	return {
		chat,
		open,
		sessions,
		getIdentity(id?: string) {
			if (id === undefined) return root.getIdentity();
			check(id);
			const value = known.get(id);
			if (!value) throw new Error("Native Session identity unavailable");
			return {
				publicSessionId: value.publicSessionId,
				workDirectoryId: value.workDirectoryId,
			};
		},
		getSnapshot: (id?: string) =>
			id === undefined
				? root.getSnapshot()
				: contexts.get(id)?.source.getSnapshot(),
		ensureSession: (id: string) =>
			use(id, ({ source }) => source.getSnapshot()),
		getFilesRevision: () =>
			Math.max(
				0,
				...[...contexts.values()].map(({ source }) =>
					source.getFilesRevision(),
				),
			),
		async createSession(sourceId: string, requestId = crypto.randomUUID()) {
			await open();
			if (!known.has(sourceId)) await sessions();
			check(sourceId);
			const descriptor = await root.createSession(sourceId, requestId);
			assertOpen();
			if (descriptor.workDirectoryId !== root.getIdentity().workDirectoryId)
				throw new Error("Native Session creation returned a foreign workspace");
			known.set(descriptor.publicSessionId, descriptor);
			notify();
			return descriptor;
		},
		selectModel: (id: string, modelId: string) =>
			use(id, ({ source }) => source.selectModel(id, modelId)),
		loadMessages: (id: string, before?: string, limit?: number) =>
			use(id, ({ source }) => source.loadMessages(id, before, limit)),
		files: () => {
			assertOpen();
			return root.files();
		},
		list: (path: string) => {
			assertOpen();
			return root.list(path);
		},
		read: (path: string) => {
			assertOpen();
			return root.read(path);
		},
		subscribe(listener: () => void) {
			listeners.add(listener);
			return () => {
				listeners.delete(listener);
			};
		},
		async dispose() {
			if (disposed) return;
			disposed = true;
			listeners.clear();
			observers.clear();
			await admission;
			const peers = [...contexts.values()];
			for (const context of peers) context.unwatch();
			try {
				await Promise.all(
					peers
						.filter(({ source }) => source !== root)
						.map(({ source }) => source.dispose()),
				);
			} finally {
				await root.dispose();
				contexts.clear();
			}
		},
	};
}
