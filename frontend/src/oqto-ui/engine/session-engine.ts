/**
 * Session engine: composes the transport and the pure projection into a
 * per-session turn stream. Owns the ephemeral in-flight draft — never the
 * durable message list; on turn end or resync the caller must drop the
 * draft and invalidate pages so store truth wins.
 */

import { type EngineEvent, type TurnDraft, projectEvent } from "./projection";
import type { ChatTransport } from "./transport";
import type { WireEvent } from "./transport-contract";

type SendKindLike = "prompt" | "steer" | "follow_up";

export type ChatDraftPart =
	| { type: "text"; id: string; text: string }
	| { type: "thinking"; id: string; text: string }
	| {
			type: "tool_call";
			id: string;
			name: string;
			status: "running" | "success" | "error";
			hasOutput: boolean;
	  }
	| { type: "error"; id: string; text: string; retrying: boolean }
	| { type: "compaction"; id: string; text: string };

export type ChatTurnDraft = { parts: ChatDraftPart[] };

export type TurnUpdate =
	| { kind: "draft"; draft: ChatTurnDraft }
	| { kind: "ended"; reason: "done" | "idle" | "error" }
	| { kind: "resync" }
	| { kind: "connection"; state: string };

export type SessionEngine = {
	bind(sessionId: string, onUpdate: (update: TurnUpdate) => void): () => void;
	send(sessionId: string, text: string, mode: SendKindLike): string;
	abort(sessionId: string): void;
};

function toRenderDraft(draft: TurnDraft): ChatTurnDraft {
	return {
		parts: draft.parts.map((part) => {
			if (part.type === "tool_call") {
				return {
					type: "tool_call" as const,
					id: part.id,
					name: part.name,
					status: part.status,
					hasOutput: part.output !== undefined,
				};
			}
			if (part.type === "compaction") {
				return { type: "compaction" as const, id: part.id, text: part.text };
			}
			if (part.type === "error") {
				return {
					type: "error" as const,
					id: part.id,
					text: part.text,
					retrying: part.retrying,
				};
			}
			return { type: part.type, id: part.id, text: part.text };
		}),
	};
}

function narrow(wire: WireEvent): EngineEvent {
	return wire as unknown as EngineEvent;
}

export function createSessionEngine(transport: ChatTransport): SessionEngine {
	const drafts = new Map<string, TurnDraft>();
	const updates = new Map<string, Set<(update: TurnUpdate) => void>>();
	let partCounter = 0;
	const nextPartId = () => `draft-p${++partCounter}`;

	function emit(sessionId: string, update: TurnUpdate): void {
		const listeners = updates.get(sessionId);
		if (!listeners) return;
		for (const listener of listeners) listener(update);
	}

	function handleWire(sessionId: string, wire: WireEvent): void {
		const outcome = projectEvent(
			drafts.get(sessionId) ?? null,
			narrow(wire),
			nextPartId,
		);
		if (outcome.kind === "draft") {
			drafts.set(sessionId, outcome.draft);
			emit(sessionId, { kind: "draft", draft: toRenderDraft(outcome.draft) });
			return;
		}
		if (outcome.kind === "turn-ended") {
			drafts.delete(sessionId);
			emit(sessionId, { kind: "ended", reason: outcome.reason });
			return;
		}
		if (outcome.kind === "resync") {
			drafts.delete(sessionId);
			emit(sessionId, { kind: "resync" });
		}
	}

	return {
		bind(sessionId, onUpdate): () => void {
			let listeners = updates.get(sessionId);
			if (!listeners) {
				listeners = new Set();
				updates.set(sessionId, listeners);
			}
			listeners.add(onUpdate);
			const detachTransport = transport.attach(sessionId, (wire) =>
				handleWire(sessionId, wire),
			);
			const observeTransport = transport.observe((connection) => {
				emit(sessionId, { kind: "connection", state: connection });
			});
			return () => {
				listeners?.delete(onUpdate);
				if (listeners && listeners.size === 0) updates.delete(sessionId);
				detachTransport();
				observeTransport();
			};
		},
		send(sessionId, text, mode): string {
			return transport.sendMessage(sessionId, mode, text);
		},
		abort(sessionId): void {
			transport.request(sessionId, "abort", undefined, 5000).catch(() => {
				// Abort is best-effort; turn end arrives via stream events or
				// the store on reconnect.
			});
		},
	};
}
