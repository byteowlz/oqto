import type { OpenCodeMessageWithParts } from "./api";

// Extended message type for Main Chat threading - includes session info.
export type ThreadedMessage = OpenCodeMessageWithParts & {
	/** Session ID this message belongs to (for Main Chat threading). */
	_sessionId?: string;
	/** Session title (for displaying session dividers). */
	_sessionTitle?: string;
	/** Whether this is the first message of a new session in the thread. */
	_isSessionStart?: boolean;
};

// Group consecutive messages from the same role.
export type MessageGroup = {
	role: "user" | "assistant";
	messages: OpenCodeMessageWithParts[];
	startIndex: number;
	/** For Main Chat: session ID this group belongs to. */
	sessionId?: string;
	/** For Main Chat: whether this group starts a new session. */
	isNewSession?: boolean;
	/** For Main Chat: session title for divider. */
	sessionTitle?: string;
};
