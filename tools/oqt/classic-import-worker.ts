import type { Context } from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import {
	buildSessionProjection,
	convertToLlm,
	type SessionEntry,
} from "@earendil-works/pi-coding-agent";
import {
	AgentDoc,
	AssistantEntry,
	type Conversation,
	type ConversationId,
	defineDoc,
	type EntryId,
	type Harness,
	InboxDoc,
	SystemEntry,
	ToolResultEntry,
	UserEntry,
} from "@earendil-works/pi-durable";
import { type ClassicSnapshot, classicSnapshot } from "./classic-import.ts";

export type ClassicImportSeed = {
	source: string;
	sourceHash: string;
	origin: string;
	leafId?: string | null;
};
/** Wire payload is raw immutable JSONL, SHA-256 of exact UTF-8 bytes, explicit namespace. */
export function validateClassicImportSeed(
	seed: ClassicImportSeed,
): ClassicSnapshot {
	const snapshot = classicSnapshot(seed.source, seed.origin);
	if (snapshot.sourceDigest !== seed.sourceHash)
		throw new Error("Classic snapshot hash mismatch");
	return snapshot;
}
export async function applyClassicImport(
	harness: Harness,
	seed: ClassicImportSeed,
	context: Context = BACKGROUND_CONTEXT,
) {
	const snapshot = validateClassicImportSeed(seed);
	const conversation = await harness.root(context);
	return importClassicSnapshot(
		{ harness, conversation, snapshot, leaf: seed.leafId },
		context,
	);
}

export const CLASSIC_PROJECTION_VERSION =
	"cdaf6c227d4718a72b59ccd02152c9614df44cfc:2";
type Mapping = {
	nativeId: string;
	conversationId: ConversationId;
	entryId: EntryId;
	projectionEntryIds: EntryId[];
};
type ImportState = {
	version: string;
	snapshotJson: string | null;
	leaf: string | null;
	mappings: Mapping[];
	watermark: EntryId | null;
	agent: string;
};
export const ClassicImportDoc = defineDoc<ImportState>({
	kind: "oq.classic-import.v1",
	version: 1,
	scope: "conversation",
	history: "latest",
	fork: "initial",
	initial: () => ({
		version: CLASSIC_PROJECTION_VERSION,
		snapshotJson: null,
		leaf: null,
		mappings: [],
		watermark: null,
		agent: "",
	}),
});
function object(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === "object" && !Array.isArray(value);
}
function content(
	value: unknown,
	allowed: readonly string[],
	strings = false,
): boolean {
	if (strings && typeof value === "string") return true;
	return (
		Array.isArray(value) &&
		value.every((block) => {
			if (!object(block) || !allowed.includes(String(block.type))) return false;
			switch (block.type) {
				case "text":
					return typeof block.text === "string";
				case "image":
					return (
						typeof block.data === "string" && typeof block.mimeType === "string"
					);
				case "thinking":
					return (
						typeof block.thinking === "string" ||
						(block.redacted === true &&
							typeof block.thinkingSignature === "string")
					);
				case "toolCall":
					return (
						typeof block.id === "string" &&
						typeof block.name === "string" &&
						object(block.arguments)
					);
				default:
					return false;
			}
		})
	);
}
function validateMessage(message: unknown) {
	if (
		!object(message) ||
		typeof message.timestamp !== "number" ||
		!Number.isFinite(message.timestamp)
	)
		throw new Error("Invalid classic message");
	switch (message.role) {
		case "system":
			if (!content(message.content ?? "", ["text"], true))
				throw new Error("Unsupported classic system content");
			break;
		case "user":
			if (!content(message.content ?? [], ["text", "image"], true))
				throw new Error("Unsupported classic user content");
			break;
		case "assistant":
			if (
				!content(message.content ?? [], ["text", "thinking", "toolCall"]) ||
				typeof message.provider !== "string" ||
				typeof message.model !== "string" ||
				typeof message.api !== "string" ||
				!object(message.usage) ||
				!["stop", "length", "toolUse", "error", "aborted"].includes(
					String(message.stopReason),
				)
			)
				throw new Error(
					"Unsupported classic assistant semantics (including pending/deferred execution)",
				);
			break;
		case "toolResult":
			if (
				!content(message.content ?? [], ["text", "image"]) ||
				typeof message.toolCallId !== "string" ||
				typeof message.toolName !== "string" ||
				typeof message.isError !== "boolean"
			)
				throw new Error("Unsupported classic tool result");
			break;
		default:
			throw new Error("Unsupported classic message role");
	}
}
/** Selected model context must describe completed historical tool exchanges,
 * never a pending external effect. Inactive native branches remain inert archive. */
export function validateClassicToolPairing(messages: readonly unknown[]): void {
	const pending = new Map<string, string>();
	const seen = new Set<string>();
	for (const value of messages) {
		if (!object(value)) throw new Error("Invalid projected classic message");
		if (value.role === "toolResult") {
			const id = value.toolCallId;
			if (
				typeof id !== "string" ||
				!pending.has(id) ||
				pending.get(id) !== value.toolName
			)
				throw new Error(
					"Classic tool pairing: unmatched, mismatched or duplicate tool result",
				);
			pending.delete(id);
			continue;
		}
		if (pending.size)
			throw new Error(
				"Classic tool pairing: missing tool result before next message",
			);
		if (value.role !== "assistant") continue;
		const calls = Array.isArray(value.content)
			? value.content.filter(
					(block) => object(block) && block.type === "toolCall",
				)
			: [];
		if ((value.stopReason === "toolUse") !== calls.length > 0)
			throw new Error("Classic tool pairing: toolUse and tool calls disagree");
		for (const call of calls) {
			if (
				!object(call) ||
				typeof call.id !== "string" ||
				!call.id ||
				typeof call.name !== "string" ||
				!call.name ||
				seen.has(call.id)
			)
				throw new Error(
					"Classic tool pairing: invalid or duplicate tool call ID",
				);
			seen.add(call.id);
			pending.set(call.id, call.name);
		}
	}
	if (pending.size)
		throw new Error(
			"Classic tool pairing: selected context has missing tool results; choose a completed source leaf",
		);
}
const supported = new Set([
	"message",
	"model_change",
	"thinking_level_change",
	"usage",
	"compaction",
	"branch_summary",
	"custom",
	"custom_message",
	"label",
	"session_info",
	"context_edit",
]);

/** Validated snapshot only. No source IO, catalog publication, scheduler, submissions,
 * extension loading or authority import. Host MUST hold its exclusive non-admitting
 * worker bootstrap gate throughout this call and must not resume archival workers.
 * Every source record has an inert native entry; the selected context is a separate
 * versioned materialization, never a claim that the native branch graph is linear.
 */
export async function importClassicSnapshot(
	options: {
		harness: Harness;
		conversation: Conversation;
		snapshot: ClassicSnapshot;
		leaf?: string | null;
	},
	context: Context,
) {
	const { harness, conversation, snapshot } = options;
	const leaf = options.leaf === undefined ? snapshot.activeLeaf : options.leaf;
	if (leaf !== null && !Object.hasOwn(snapshot.entryDigests, leaf))
		throw new Error("Unknown selected classic leaf");
	const byId = new Map(snapshot.records.map((r) => [r.id, r]));
	for (const r of snapshot.records) {
		if (!supported.has(String(r.type)))
			throw new Error(
				`Unsupported classic semantics: ${JSON.stringify(r.type)}`,
			);
		if (!Number.isFinite(Date.parse(String(r.timestamp))))
			throw new Error("Invalid classic timestamp");
		if (r.type === "message") validateMessage(r.message);
		if (
			r.type === "custom_message" &&
			(typeof r.customType !== "string" ||
				typeof r.display !== "boolean" ||
				!content(r.content ?? [], ["text", "image"], true))
		)
			throw new Error("Unsupported classic custom message");
		if (
			r.type === "model_change" &&
			(typeof r.provider !== "string" || typeof r.modelId !== "string")
		)
			throw new Error("Invalid classic model change");
		if (
			r.type === "thinking_level_change" &&
			typeof r.thinkingLevel !== "string"
		)
			throw new Error("Invalid classic thinking level");
		if (
			r.type === "branch_summary" &&
			(typeof r.summary !== "string" || !byId.has(r.fromId))
		)
			throw new Error("Invalid classic branch summary reference");
		if (r.type === "label" && !byId.has(r.targetId))
			throw new Error("Invalid classic label reference");
		if (
			r.type === "context_edit" &&
			(!byId.has(r.targetId) ||
				!(
					r.replacement === null ||
					(typeof r.replacement === "object" &&
						r.replacement !== null &&
						!Array.isArray(r.replacement) &&
						Object.hasOwn(r.replacement, "content"))
				))
		)
			throw new Error("Unsupported classic context edit");
		if (r.type === "context_edit") {
			const target = byId.get(r.targetId);
			if (
				!target ||
				!(
					target.type === "custom_message" ||
					(target.type === "message" &&
						object(target.message) &&
						["user", "assistant", "toolResult"].includes(
							String(target.message.role),
						))
				)
			)
				throw new Error("Unsupported classic context edit target");
			let ancestor = r.parentId;
			while (ancestor !== null && ancestor !== r.targetId)
				ancestor = byId.get(ancestor)?.parentId ?? null;
			if (ancestor === null)
				throw new Error("Classic context edit target is not an ancestor");
			if (
				r.replacement !== null &&
				(!object(r.replacement) ||
					!content(
						r.replacement.content,
						["text", "image", "thinking", "toolCall"],
						true,
					))
			)
				throw new Error("Unsupported classic context edit content");
		}
		if (r.type === "compaction") {
			if (typeof r.summary !== "string" || typeof r.tokensBefore !== "number")
				throw new Error("Invalid classic compaction");
			if (r.systemMessage !== undefined) {
				validateMessage(r.systemMessage);
				if (!object(r.systemMessage) || r.systemMessage.role !== "system")
					throw new Error("Invalid classic compaction checkpoint");
			}
			let ancestor = r.id;
			while (ancestor !== null && ancestor !== r.firstKeptEntryId)
				ancestor = byId.get(ancestor)?.parentId ?? null;
			if (ancestor === null)
				throw new Error("Unsupported classic compaction kept reference");
		}
	}
	// Only the pinned pure builder is used, never SessionManager or mutable loaders.
	const projection = buildSessionProjection(
		snapshot.records as unknown as SessionEntry[],
		leaf,
	);
	const projected = {
		messages: projection.messages,
		model: projection.model,
		thinkingLevel: projection.thinkingLevel,
	};
	const model = convertToLlm(projected.messages);
	validateClassicToolPairing(model);
	return harness.commit(async (tx) => {
		const state = await tx.doc(ClassicImportDoc, conversation.id);
		const newest = (
			await tx.scanEntries({ conversationId: conversation.id }, 1)
		).items[0];
		const agent = JSON.stringify(await tx.doc(AgentDoc, conversation.id));
		const tasks = await tx.scanTasks({ conversationId: conversation.id }, 1000);
		if (
			tasks.next ||
			(await tx.doc(InboxDoc, conversation.id)).items.length ||
			tasks.items.some((t) => t.state.status !== "terminal")
		)
			throw new Error("Classic import requires an unscheduled idle worker");
		if (state.snapshotJson) {
			const previous = JSON.parse(state.snapshotJson) as ClassicSnapshot;
			if (state.version !== CLASSIC_PROJECTION_VERSION)
				throw new Error("Classic projection version conflict");
			if (newest?.id !== state.watermark || agent !== state.agent)
				throw new Error("Classic destination diverged");
			if (
				previous.sourceId !== snapshot.sourceId ||
				previous.sourceDigest !== snapshot.sourceDigest ||
				previous.sourceText !== snapshot.sourceText ||
				leaf !== state.leaf
			)
				throw new Error(
					"Classic immutable import changed; require a new snapshot destination",
				);
			return {
				kind: "unchanged" as const,
				conversationId: conversation.id,
				watermark: state.watermark,
				mappings: JSON.parse(JSON.stringify(state.mappings)) as Mapping[],
			};
		} else if (newest)
			throw new Error("Classic import requires an empty target");
		for (const record of snapshot.records) {
			const entry = await tx.appendEntry(conversation.id, {
				kind: "oq.classic-archive.v1",
				data: { sourceId: snapshot.sourceId, record },
			});
			state.mappings.push({
				nativeId: String(record.id),
				conversationId: conversation.id,
				entryId: entry.id,
				projectionEntryIds: [],
			});
		}
		const checkpoint = await tx.appendEntry(conversation.id, {
			kind: "oq.classic-projection.v2",
			head: "self",
			data: {
				sourceId: snapshot.sourceId,
				sourceDigest: snapshot.sourceDigest,
				leaf,
				version: CLASSIC_PROJECTION_VERSION,
				recordedSettings: {
					model: projected.model,
					thinkingLevel: projected.thinkingLevel,
				},
				classicContext: JSON.parse(JSON.stringify(projected)),
			},
		});
		// Native role views, not extension renderer parity. Explicitly hidden
		// custom messages remain model-visible under an inert kind that stock TUI
		// does not render. No submit/task machinery participates.
		// Source identity comes from the canonical projection, never text/order matching.
		let watermark = checkpoint.id;
		const mappings = new Map(
			state.mappings.map((mapping) => [mapping.nativeId, mapping]),
		);
		for (const selected of projection.entries) {
			const mapping = mappings.get(selected.sourceEntry.id);
			if (!mapping)
				throw new Error("Classic projection source mapping unavailable");
			for (const message of convertToLlm(selected.messages)) {
				const hidden =
					selected.sourceEntry.type === "custom_message" &&
					selected.sourceEntry.display === false;
				const kind = hidden
					? "oq.classic-hidden-context.v2"
					: message.role === "user"
						? UserEntry.kind
						: message.role === "assistant"
							? AssistantEntry.kind
							: message.role === "toolResult"
								? ToolResultEntry.kind
								: message.role === "system"
									? SystemEntry.kind
									: undefined;
				if (!kind)
					throw new Error("Unsupported classic native materialization role");
				const materialized = await tx.appendEntry(conversation.id, {
					kind,
					model: [message],
					data: {
						provenance: {
							sourceId: snapshot.sourceId,
							sourceEntryId: selected.sourceEntry.id,
							version: CLASSIC_PROJECTION_VERSION,
						},
						...(message.role === "toolResult" ? { diagnostics: [] } : {}),
					},
				});
				mapping.projectionEntryIds.push(materialized.id);
				watermark = materialized.id;
			}
		}
		state.snapshotJson = JSON.stringify(snapshot);
		state.version = CLASSIC_PROJECTION_VERSION;
		state.leaf = leaf;
		state.watermark = watermark;
		state.agent = agent;
		return {
			kind: "imported" as const,
			conversationId: conversation.id,
			watermark,
			mappings: JSON.parse(JSON.stringify(state.mappings)) as Mapping[],
		};
	}, context);
}
