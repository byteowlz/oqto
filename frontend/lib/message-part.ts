/** Render-facing agent message part, independent from transport clients. */
export type MessagePart = {
	id: string;
	sessionID: string;
	messageID: string;
	type:
		| "text"
		| "tool"
		| "file"
		| "reasoning"
		| "step-start"
		| "step-finish"
		| "snapshot"
		| "patch"
		| "agent"
		| "retry"
		| "compaction"
		| "subtask";
	text?: string;
	tool?: string;
	callID?: string;
	mime?: string;
	url?: string;
	filename?: string;
	state?: {
		status: "pending" | "running" | "completed" | "error";
		input?: Record<string, unknown>;
		output?: string;
		title?: string;
		time?: { start: number; end?: number };
	};
	time?: { start?: number; end?: number };
	metadata?: Record<string, unknown>;
};
