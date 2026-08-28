/** Writable-chat handle contract between views and platform adapters. */

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

export type ChatTurnUpdate =
	| { kind: "draft"; draft: ChatTurnDraft }
	| { kind: "ended"; reason: "done" | "idle" | "error" }
	| { kind: "resync" }
	| { kind: "connection"; state: string };

export type ChatEngineHandle = {
	bind(
		sessionId: string,
		onUpdate: (update: ChatTurnUpdate) => void,
	): () => void;
	send(
		sessionId: string,
		text: string,
		mode: "prompt" | "steer" | "follow_up",
	): string;
	abort(sessionId: string): void;
};
