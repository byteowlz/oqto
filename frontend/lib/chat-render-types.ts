import type { A2UIMessage } from "@/lib/a2ui/types";
import type { Part, Role, Sender, Usage } from "@/lib/canonical-types";

export type CompactionPart = { type: "compaction"; id: string; text: string };

export type ErrorPart = {
	type: "error";
	id: string;
	text: string;
	retryAttempt?: number;
	retryMax?: number;
	retrying?: boolean;
};

export type DisplayPart = Part | CompactionPart | ErrorPart;

export type A2UISurfaceState = {
	surfaceId: string;
	sessionId: string;
	messages: A2UIMessage[];
	blocking: boolean;
	requestId?: string;
	createdAt: Date;
	anchorMessageId?: string;
	answered?: boolean;
	answeredAction?: string;
	answeredAt?: Date;
};

/** Transport-neutral message contract consumed by canonical Chat renderers. */
export type DisplayMessage = {
	id: string;
	role: Role;
	parts: DisplayPart[];
	timestamp: number;
	parentId?: string | null;
	branchId?: string | null;
	isStreaming?: boolean;
	usage?: Usage;
	clientId?: string;
	model?: string | null;
	provider?: string | null;
	sender?: Sender;
};
