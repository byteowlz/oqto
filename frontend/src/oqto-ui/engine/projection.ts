/**
 * Pure projection: wire events -> turn draft updates.
 *
 * The draft is ephemeral render decoration for the currently streaming
 * turn. Durable truth stays in the timeline page cache (oqto-log pages);
 * a turn-ended/resync outcome tells the caller to drop the draft and
 * invalidate pages. Nothing here touches React, sockets, or storage.
 */

export type JsonValue =
	| string
	| number
	| boolean
	| null
	| JsonValue[]
	| { [key: string]: JsonValue };

export type DraftPart =
	| { type: "text"; id: string; text: string }
	| { type: "thinking"; id: string; text: string }
	| {
			type: "tool_call";
			id: string;
			toolCallId: string;
			name: string;
			input?: JsonValue;
			output?: JsonValue;
			status: "running" | "success" | "error";
	  }
	| {
			type: "error";
			id: string;
			text: string;
			retrying: boolean;
			retryAttempt?: number;
			retryMax?: number;
	  }
	| { type: "compaction"; id: string; text: string; pending?: boolean };

export type TurnDraft = {
	sessionId: string;
	parts: DraftPart[];
	startedAt: number;
};

export type EngineEvent = {
	event: string;
	session_id?: string;
	role?: string;
	delta?: string;
	tool_call_id?: string;
	name?: string;
	input?: JsonValue;
	output?: JsonValue;
	is_error?: boolean;
	tool_call?: { id: string; name: string; input?: JsonValue };
	attempt?: number;
	max_attempts?: number;
	error?: string;
	success?: boolean;
	tokens_before?: number;
};

export type ProjectionOutcome =
	| { kind: "draft"; draft: TurnDraft }
	| { kind: "turn-ended"; reason: "done" | "idle" | "error" }
	| { kind: "resync" }
	| { kind: "ignored" };

type Reducer = (
	draft: TurnDraft,
	event: EngineEvent,
	nextPartId: () => string,
) => TurnDraft;

function withParts(draft: TurnDraft, parts: DraftPart[]): TurnDraft {
	return { ...draft, parts };
}

function appendDelta(
	draft: TurnDraft,
	partType: "text" | "thinking",
	delta: string,
	nextPartId: () => string,
): TurnDraft {
	const last = draft.parts.at(-1);
	if (last && last.type === partType) {
		const merged = { ...last, text: last.text + delta };
		return withParts(draft, [...draft.parts.slice(0, -1), merged]);
	}
	return withParts(draft, [
		...draft.parts,
		{ type: partType, id: nextPartId(), text: delta },
	]);
}

type ToolCallUpdate = {
	toolCallId: string;
	name?: string;
	input?: JsonValue;
	output?: JsonValue;
	status?: "running" | "success" | "error";
};

function upsertToolCall(
	draft: TurnDraft,
	update: ToolCallUpdate,
	nextPartId: () => string,
): TurnDraft {
	const index = draft.parts.findIndex(
		(part) =>
			part.type === "tool_call" && part.toolCallId === update.toolCallId,
	);
	if (index >= 0) {
		const existing = draft.parts[index];
		if (existing.type !== "tool_call") return draft;
		const merged: DraftPart = {
			...existing,
			name: update.name || existing.name,
			input: update.input ?? existing.input,
			output: update.output ?? existing.output,
			status: update.status ?? existing.status,
		};
		const parts = [...draft.parts];
		parts[index] = merged;
		return withParts(draft, parts);
	}
	return withParts(draft, [
		...draft.parts,
		{
			type: "tool_call",
			id: nextPartId(),
			toolCallId: update.toolCallId,
			name: update.name ?? "tool",
			input: update.input,
			output: update.output,
			status: update.status ?? "running",
		},
	]);
}

function upsertRetryError(
	draft: TurnDraft,
	event: EngineEvent,
	nextPartId: () => string,
): TurnDraft {
	const reason =
		typeof event.error === "string" && event.error.trim().length > 0
			? event.error.trim()
			: "The model request failed";
	const suffix =
		event.attempt && event.max_attempts
			? ` — retrying (${event.attempt}/${event.max_attempts})…`
			: " — retrying…";
	const text = `${reason}${suffix}`;
	const index = draft.parts.findIndex(
		(part) => part.type === "error" && part.retrying,
	);
	const part: DraftPart = {
		type: "error",
		id: index >= 0 ? draft.parts[index].id : nextPartId(),
		text,
		retrying: true,
		retryAttempt: event.attempt,
		retryMax: event.max_attempts,
	};
	if (index >= 0) {
		const parts = [...draft.parts];
		parts[index] = part;
		return withParts(draft, parts);
	}
	return withParts(draft, [...draft.parts, part]);
}

const reducers: Readonly<Record<string, Reducer>> = {
	"stream.text_delta": (draft, event, nextPartId) =>
		event.delta ? appendDelta(draft, "text", event.delta, nextPartId) : draft,
	"stream.thinking_delta": (draft, event, nextPartId) =>
		event.delta
			? appendDelta(draft, "thinking", event.delta, nextPartId)
			: draft,
	"stream.tool_call_start": (draft, event, nextPartId) =>
		event.tool_call_id
			? upsertToolCall(
					draft,
					{
						toolCallId: event.tool_call_id,
						name: event.name,
						status: "running",
					},
					nextPartId,
				)
			: draft,
	"stream.tool_call_end": (draft, event, nextPartId) =>
		event.tool_call?.id
			? upsertToolCall(
					draft,
					{
						toolCallId: event.tool_call.id,
						name: event.tool_call.name,
						input: event.tool_call.input,
						status: "running",
					},
					nextPartId,
				)
			: draft,
	"tool.start": (draft, event, nextPartId) =>
		event.tool_call_id
			? upsertToolCall(
					draft,
					{
						toolCallId: event.tool_call_id,
						name: event.name,
						input: event.input,
						status: "running",
					},
					nextPartId,
				)
			: draft,
	"tool.end": (draft, event, nextPartId) =>
		event.tool_call_id
			? upsertToolCall(
					draft,
					{
						toolCallId: event.tool_call_id,
						name: event.name,
						output: event.output,
						status: event.is_error ? "error" : "success",
					},
					nextPartId,
				)
			: draft,
	"retry.start": upsertRetryError,
	"retry.end": (draft) =>
		withParts(
			draft,
			draft.parts.filter((part) => !(part.type === "error" && part.retrying)),
		),
	"compact.start": (draft, _event, nextPartId) =>
		withParts(draft, [
			...draft.parts,
			{
				type: "compaction",
				id: nextPartId(),
				text: "Compacting context...",
				pending: true,
			},
		]),
	"compact.end": (draft, event, nextPartId) => {
		const text = event.success
			? event.tokens_before
				? `Context compacted (${
						event.tokens_before >= 1000
							? `${(event.tokens_before / 1000).toFixed(1)}K`
							: event.tokens_before
					} tokens summarized)`
				: "Context compacted"
			: event.error || "Compaction failed";
		const part: DraftPart = event.success
			? { type: "compaction", id: nextPartId(), text }
			: { type: "error", id: nextPartId(), text, retrying: false };
		const index = draft.parts.findIndex(
			(candidate) => candidate.type === "compaction" && candidate.pending,
		);
		if (index >= 0) {
			const parts = [...draft.parts];
			parts[index] = part;
			return withParts(draft, parts);
		}
		return withParts(draft, [...draft.parts, part]);
	},
};

const turnEndReasons: Readonly<
	Record<string, "done" | "idle" | "error" | undefined>
> = {
	"stream.done": "done",
	"stream.message_end": "done",
	"agent.idle": "idle",
	"agent.error": "error",
};

function isAssistantRole(role: string | undefined): boolean {
	return !role || role === "assistant" || role === "agent";
}

/**
 * Project one wire event onto the current draft. Pure; returns the
 * outcome the stateful glue applies to the page cache.
 */
export function projectEvent(
	draft: TurnDraft | null,
	event: EngineEvent,
	nextPartId: () => string,
	now: () => number = Date.now,
): ProjectionOutcome {
	if (event.event === "stream.resync_required") return { kind: "resync" };

	const endReason = turnEndReasons[event.event];
	if (endReason) return { kind: "turn-ended", reason: endReason };

	if (event.event === "stream.message_start") {
		// Runner emits message_start for user echoes and tool-result
		// messages too; only assistant turns get a streaming draft.
		if (!isAssistantRole(event.role)) return { kind: "ignored" };
		return {
			kind: "draft",
			draft: draft ?? {
				sessionId: event.session_id ?? "",
				parts: [],
				startedAt: now(),
			},
		};
	}

	const reducer = reducers[event.event];
	if (!reducer) return { kind: "ignored" };
	const base = draft ?? {
		sessionId: event.session_id ?? "",
		parts: [],
		startedAt: now(),
	};
	return { kind: "draft", draft: reducer(base, event, nextPartId) };
}
