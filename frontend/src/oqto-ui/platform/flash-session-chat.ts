import type { ConversationView } from "@oqto-flash/types";
import { FlashWorkerClient } from "@oqto-flash/worker-client";
import type { JsonValue } from "../engine/projection";
import type {
	ChatDraftPart,
	ChatEngineHandle,
	ChatTurnUpdate,
} from "./chat-contract";
import type { ChatMessage, ChatMessagePart, MessagePage } from "./contracts";

/** Public identities are committed native metadata, not numeric harness IDs. */
export function flashIdentity(value: unknown): {
	publicSessionId: string;
	workDirectoryId: string;
} {
	const identity = record(value);
	if (
		!identity ||
		typeof identity.publicSessionId !== "string" ||
		typeof identity.workDirectoryId !== "string"
	)
		throw new Error("Native browser identity is unavailable");
	return {
		publicSessionId: identity.publicSessionId,
		workDirectoryId: identity.workDirectoryId,
	};
}
type NativeFields = {
	type?: unknown;
	role?: unknown;
	text?: unknown;
	thinking?: unknown;
	id?: unknown;
	name?: unknown;
	arguments?: JsonValue;
	content?: unknown;
	timestamp?: unknown;
	toolCallId?: unknown;
	toolName?: unknown;
	isError?: unknown;
	run?: unknown;
	taskId?: unknown;
	generation?: unknown;
	attempt?: unknown;
	tools?: unknown;
	callId?: unknown;
	status?: unknown;
	entry?: unknown;
	output?: JsonValue;
	retry?: unknown;
	error?: unknown;
	message?: unknown;
	stopReason?: unknown;
	errorMessage?: unknown;
	publicSessionId?: unknown;
	workDirectoryId?: unknown;
	model?: unknown;
	modelId?: unknown;
};
function record(value: unknown): NativeFields {
	return value && typeof value === "object" ? (value as NativeFields) : {};
}
function parts(content: unknown, key: string): ChatMessagePart[] {
	if (typeof content === "string")
		return [{ type: "text", id: `${key}:text:0`, text: content }];
	if (!Array.isArray(content)) return [];
	return content.flatMap((value, slot): ChatMessagePart[] => {
		const part = record(value);
		const id = `${key}:block:${slot}`; // Native content-block identity, not visible-message order.
		if (part.type === "text")
			return [{ type: "text", id, text: String(part.text ?? "") }];
		if (part.type === "thinking")
			return [{ type: "thinking", id, text: String(part.thinking ?? "") }];
		if (part.type === "toolCall" && typeof part.id === "string")
			return [
				{
					type: "tool_call",
					id: `${key}:call:${part.id}`,
					toolCallId: part.id,
					name: String(part.name),
					input: part.arguments as JsonValue,
					status: "pending",
				},
			];
		return [];
	});
}

/** Pure projection: entry and message-slot identities survive repeat snapshots/reload. */
export function flashHistory(view: ConversationView): ChatMessage[] {
	const results = new Map<string, boolean>();
	for (const entry of view.entries)
		for (const message of entry.model ?? [])
			if (message.role === "toolResult")
				results.set(message.toolCallId, message.isError === true);
	return view.entries.flatMap((entry) =>
		(entry.model ?? []).flatMap((native, slot): ChatMessage[] => {
			const message = record(native);
			if (!["user", "assistant", "toolResult"].includes(String(message.role)))
				return [];
			const id = `${view.conversation.id}:${entry.id}:message:${slot}`;
			const projected = parts(message.content, id).map((part) =>
				part.type === "tool_call"
					? {
							...part,
							status: results.has(part.toolCallId)
								? results.get(part.toolCallId)
									? ("error" as const)
									: ("success" as const)
								: ("pending" as const),
						}
					: part,
			);
			if (
				message.role === "assistant" &&
				typeof message.errorMessage === "string" &&
				message.errorMessage
			) {
				projected.push({
					type: "text",
					id: `${id}:error`,
					text: `Provider error: ${message.errorMessage}`,
					format: "plain",
				});
			}
			const content = projected
				.filter((part) => part.type === "text")
				.map((part) => part.text)
				.join("\n");
			return [
				{
					id,
					author:
						message.role === "assistant"
							? "agent"
							: message.role === "toolResult"
								? "tool"
								: "user",
					content,
					parts:
						message.role === "toolResult"
							? [
									{
										type: "tool_result",
										id: `${id}:result`,
										toolCallId: String(message.toolCallId),
										name: String(message.toolName ?? ""),
										output: content,
										isError: message.isError === true,
									},
								]
							: projected,
					time:
						typeof message.timestamp === "number"
							? new Date(message.timestamp).toISOString()
							: "",
				},
			];
		}),
	);
}

export type FlashChatOptions = {
	binding: Parameters<FlashWorkerClient["open"]>[1];
	apiKey: string;
	workspace?: string;
};
export function createFlashSessionChat(
	options: FlashChatOptions,
	publicSessionId?: string,
) {
	let snapshot: ConversationView | undefined;
	let identity: ReturnType<typeof flashIdentity> | undefined;
	const getIdentity = () => {
		if (!identity) throw new Error("Browser runtime is not open");
		return identity;
	};
	let disposed = false;
	let opening: Promise<void> | undefined;
	const listeners = new Set<() => void>();
	const pendingSelections = new Set<() => void>();
	const updates = new Set<(update: ChatTurnUpdate) => void>();
	const emit = (update: ChatTurnUpdate) => {
		for (const listener of updates) listener(update);
	};
	const check = (id: string) => {
		if (id !== getIdentity().publicSessionId)
			throw new Error(`Unknown Flash adapter session: ${id}`);
		if (disposed) throw new Error("Flash chat disposed");
	};
	const client = new FlashWorkerClient((view) => {
		if (disposed) return;
		const wasRunning = Boolean(record(snapshot?.docs["pi.live"]).run);
		snapshot = view;
		emit({ kind: "resync" });
		const live = record(view.docs["pi.live"]);
		// Idle metadata commits (for example a model change) are not new turns.
		if (!live.run && !wasRunning) {
			for (const listener of listeners) listener();
			return;
		}
		const generation = record(live.generation);
		const key = `${view.conversation.id}:run:${record(live.run).taskId}:attempt:${generation.attempt}`;
		const draft: ChatDraftPart[] = parts(
			record(generation.message).content,
			key,
		).flatMap((part): ChatDraftPart[] => {
			if (part.type === "tool_call") return [{ ...part, status: "running" }];
			if (part.type === "text" || part.type === "thinking") return [part];
			return [];
		});
		for (const value of Array.isArray(live.tools) ? live.tools : []) {
			const tool = record(value);
			const existing = draft.findIndex(
				(part) => part.type === "tool_call" && part.toolCallId === tool.callId,
			);
			if (existing >= 0) draft.splice(existing, 1);
			draft.push({
				type: "tool_call",
				id: `${view.conversation.id}:call:${tool.callId}`,
				toolCallId: String(tool.callId),
				name: String(tool.name),
				output: tool.output as JsonValue,
				status:
					tool.status === "done"
						? tool.entry
							? "success"
							: "error"
						: "running",
			});
		}
		const retry = record(generation.retry);
		if (retry.error)
			draft.push({
				type: "error",
				id: `${key}:retry`,
				text: String(retry.error),
				retrying: true,
			});
		emit({ kind: "draft", draft: { parts: draft } });
		if (wasRunning && !live.run) {
			const lastAssistant = view.entries
				.flatMap((entry) => entry.model ?? [])
				.filter((message) => record(message).role === "assistant")
				.at(-1);
			const failed = record(lastAssistant).stopReason === "error";
			emit({ kind: "ended", reason: failed ? "error" : "done" });
		}
		for (const listener of listeners) listener();
	});
	const reportError = (error: unknown) => {
		if (disposed) return;
		emit({
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
		emit({ kind: "ended", reason: "error" });
	};
	const open = (): Promise<void> => {
		if (disposed) return Promise.reject(new Error("Flash chat disposed"));
		opening ??= client
			.open(
				options.workspace ?? "default",
				options.binding,
				options.apiKey,
				publicSessionId,
			)
			.then((value) => {
				identity = flashIdentity(value);
				// Custom identity metadata is read by the owner, not mounted in the
				// built-in ConversationView docs. The immutable peer supplies its map.
			})
			.catch((error) => {
				opening = undefined;
				reportError(error);
				throw error;
			});
		return opening;
	};
	const sendRequest = (
		id: string,
		text: string,
		mode: Parameters<ChatEngineHandle["send"]>[2],
		requestId = crypto.randomUUID(),
	) => {
		check(id);
		void open()
			.then(() =>
				client.submit(
					text,
					requestId,
					mode === "prompt"
						? "reject"
						: mode === "steer"
							? "steer"
							: "followUp",
				),
			)
			.catch(reportError);
		return requestId;
	};
	const chat: ChatEngineHandle = {
		bind(id, listener) {
			check(id);
			updates.add(listener);
			listener({ kind: "resync" });
			return () => {
				updates.delete(listener);
			};
		},
		send: sendRequest,
		abort(id) {
			check(id);
			void client.abort().catch(reportError);
		},
	};
	return {
		chat,
		open,
		sendRequest,
		getSnapshot: () => snapshot,
		getFilesRevision: () => client.filesRevision ?? 0,
		async sessions() {
			await open();
			return client.sessions();
		},
		async createSession(sourceSessionId: string, requestId: string) {
			await open();
			return client.createSession(sourceSessionId, requestId);
		},
		async selectModel(sessionId: string, modelId: string): Promise<void> {
			check(sessionId);
			await client.selectModel(modelId);
			// ACK is a durable commit; wait for this observer's native projection
			// before the caller invalidates the shell query. Never project a local echo.
			await new Promise<void>((resolve, reject) => {
				const cleanup = () => {
					globalThis.clearTimeout(timer);
					listeners.delete(observe);
					pendingSelections.delete(cancel);
				};
				const cancel = () => {
					cleanup();
					reject(new Error("Browser runtime closed during model selection"));
				};
				const timer = globalThis.setTimeout(() => {
					cleanup();
					reject(
						new Error(
							"Model committed but its native snapshot did not arrive; reopen to inspect the selection",
						),
					);
				}, 30000);
				const observe = () => {
					if (
						record(record(snapshot?.docs["pi.agent"]).model).modelId !== modelId
					)
						return;
					cleanup();
					resolve();
				};
				pendingSelections.add(cancel);
				listeners.add(observe);
				if (disposed) cancel();
				else observe();
			});
		},
		getIdentity,
		subscribe(listener: () => void) {
			listeners.add(listener);
			return () => {
				listeners.delete(listener);
			};
		},
		async loadMessages(
			sessionId: string,
			before?: string,
			limit = 100,
		): Promise<MessagePage> {
			check(sessionId);
			await open();
			if (!snapshot)
				throw new Error("Flash runtime has not published a native snapshot");
			const messages = flashHistory(snapshot);
			const end =
				before === undefined
					? messages.length
					: messages.findIndex((message) => message.id === before);
			if (end < 0)
				throw new Error(
					"Stale or foreign Flash history cursor; reload native history",
				);
			if (!Number.isInteger(limit) || limit < 1)
				throw new Error("History limit must be a positive integer");
			const start = Math.max(0, end - limit);
			const page = messages.slice(start, end);
			return {
				sessionId,
				messages: page,
				hasMore: start > 0,
				nextBefore: start > 0 ? page[0].id : null,
			};
		},
		async files() {
			await open();
			return client.files();
		},
		async list(path: string) {
			await open();
			return client.list(path);
		},
		async read(path: string) {
			await open();
			return client.read(path);
		},
		async dispose() {
			disposed = true;
			for (const cancel of pendingSelections) cancel();
			updates.clear();
			listeners.clear();
			await client.close();
		},
	};
}
