"use client";

/**
 * Re-export usePiChat and related utilities from the main-chat feature module.
 * This file exists for backward compatibility - new code should import from
 * @/features/main-chat/hooks directly.
 */

// Main hook
export { usePiChat } from "@/features/main-chat/hooks";

// Scroll position utilities
export {
	getCachedScrollPosition,
	setCachedScrollPosition,
} from "@/features/main-chat/hooks";

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
} from "@/features/main-chat/hooks";
