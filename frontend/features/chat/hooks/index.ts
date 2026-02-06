/**
 * Default chat feature hooks.
 *
 * Chat sessions use the multiplexed WebSocket via useChat.
 * The old per-session WebSocket implementation has been removed.
 */

// Main composition hook (uses multiplexed WebSocket)
export { useChat } from "./useChat";

// Navigation hook

// Cache utilities
export {
	getCachedScrollPosition,
	setCachedScrollPosition,
	readCachedSessionMessages,
	writeCachedSessionMessages,
	clearCachedSessionMessages,
} from "./cache";

// Message utilities
export {
	convertToDisplayMessages,
	convertSessionMessagesToDisplay,
	mergeServerMessages,
} from "./message-utils";

// Types
export type {
	PiEventType,
	PiStreamEvent,
	PiMessagePart,
	PiDisplayMessage,
	PiSendMode,
	PiSendOptions,
	UsePiChatOptions,
	UsePiChatReturn,
	RawPiMessage,
	BatchedUpdateState,
	SessionMessageCacheEntry,
	WsConnectionState,
	AgentState,
} from "./types";
