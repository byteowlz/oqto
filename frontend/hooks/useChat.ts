"use client";

/**
 * Re-export useChat and related utilities from the default-chat feature module.
 */

// Main hook
export { useChat } from "@/features/chat/hooks";

// Scroll position utilities
export {
	getCachedScrollPosition,
	setCachedScrollPosition,
} from "@/features/chat/hooks";

// Types
export type {
	DisplayPart,
	DisplayMessage,
	SendMode,
	SendOptions,
	UseChatOptions,
	UseChatReturn,
} from "@/features/chat/hooks";
