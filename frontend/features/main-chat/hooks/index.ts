/**
 * Main chat feature hooks.
 *
 * As of the Pi session refactor, usePiChat now uses the multiplexed WebSocket
 * via usePiChatV2. The old per-session WebSocket implementation has been removed.
 */

// Main composition hook (uses multiplexed WebSocket)
export { usePiChatV2 as usePiChat } from "./usePiChatV2";

// Navigation hook
export { useMainChatNavigation } from "./useMainChatNavigation";

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
	PiState,
} from "./types";
