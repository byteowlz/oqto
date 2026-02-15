import type { MessageWithParts } from "./api";

// Extended message type for Default Chat threading - includes session info.
export type ThreadedMessage = MessageWithParts & {
	/** Session ID this message belongs to (for Default Chat threading). */
	_sessionId?: string;
	/** Session title (for displaying session dividers). */
	_sessionTitle?: string;
	/** Whether this is the first message of a new session in the thread. */
	_isSessionStart?: boolean;
};

// Group consecutive messages from the same role.
export type MessageGroup = {
	role: "user" | "assistant";
	messages: MessageWithParts[];
	startIndex: number;
	/** For Default Chat: session ID this group belongs to. */
	sessionId?: string;
	/** For Default Chat: whether this group starts a new session. */
	isNewSession?: boolean;
	/** For Default Chat: session title for divider. */
	sessionTitle?: string;
};
