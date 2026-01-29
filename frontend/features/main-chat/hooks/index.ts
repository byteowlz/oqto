/**
 * Main chat feature hooks.
 */

// Main composition hook
export { usePiChat } from "./usePiChat";

// Sub-hooks for advanced usage
export { usePiChatStreaming } from "./usePiChatStreaming";
export { usePiChatHistory, usePiChatHistoryEffects } from "./usePiChatHistory";
export {
	usePiChatCore,
	usePiChatSessionEffects,
	usePiChatStreamingFallback,
	usePiChatInit,
} from "./usePiChatCore";

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
