/**
 * Unified hook for navigating to specific messages in chat views.
 * Used by both sidebar session search and in-chat search.
 */

import { useCallback, useEffect, useRef } from "react";

export type MessageNavigationTarget = {
	/** Message ID to scroll to */
	messageId: string;
	/** Line number in session file (for hstry results) */
	lineNumber?: number;
	/** Session ID if navigating to a different session */
	sessionId?: string;
	/** Whether this is a Main Chat (Pi) session */
	isMainChat?: boolean;
};

export type UseMessageNavigationOptions = {
	/** Ref to the messages container element */
	containerRef: React.RefObject<HTMLElement>;
	/** Current messages array (used to check if message exists) */
	messages: Array<{ id: string }>;
	/** Current visible message count (for virtualization) */
	visibleCount?: number;
	/** Setter to expand visible messages */
	setVisibleCount?: (count: number | ((prev: number) => number)) => void;
	/** Callback when navigation completes */
	onNavigationComplete?: () => void;
};

/**
 * Hook that provides message navigation functionality.
 * Handles scrolling, highlighting, and visibility expansion.
 */
export function useMessageNavigation(options: UseMessageNavigationOptions) {
	const {
		containerRef,
		messages,
		visibleCount = messages.length,
		setVisibleCount,
		onNavigationComplete,
	} = options;

	const pendingNavigationRef = useRef<string | null>(null);

	/**
	 * Scroll to a message by ID with highlight animation.
	 * Returns true if message was found and scrolled to.
	 */
	const scrollToMessage = useCallback(
		(messageId: string): boolean => {
			if (!containerRef.current) return false;

			const messageEl = containerRef.current.querySelector(
				`[data-message-id="${messageId}"]`,
			);

			if (!messageEl) {
				// Message not in DOM yet, may need to expand visible count
				return false;
			}

			// Scroll to the message
			requestAnimationFrame(() => {
				messageEl.scrollIntoView({ behavior: "smooth", block: "center" });
				// Add highlight animation
				messageEl.classList.add("search-highlight");
				setTimeout(() => {
					messageEl.classList.remove("search-highlight");
				}, 2000);
			});

			onNavigationComplete?.();
			return true;
		},
		[containerRef, onNavigationComplete],
	);

	/**
	 * Navigate to a message, expanding visible count if needed.
	 */
	const navigateToMessage = useCallback(
		(target: MessageNavigationTarget) => {
			const { messageId } = target;

			// First, try to scroll directly
			if (scrollToMessage(messageId)) {
				return;
			}

			// Message not visible, try to find its index and expand
			const messageIndex = messages.findIndex((m) => m.id === messageId);
			if (messageIndex !== -1 && setVisibleCount) {
				// Calculate how many messages we need to show
				const messagesFromEnd = messages.length - messageIndex;
				if (messagesFromEnd > visibleCount) {
					// Expand to include this message plus some buffer
					setVisibleCount(messagesFromEnd + 10);
					// Store pending navigation to execute after render
					pendingNavigationRef.current = messageId;
				}
			}
		},
		[messages, visibleCount, setVisibleCount, scrollToMessage],
	);

	// Handle pending navigation after visible count changes
	useEffect(() => {
		if (!pendingNavigationRef.current) return;

		const messageId = pendingNavigationRef.current;
		// Use requestAnimationFrame to wait for DOM update
		requestAnimationFrame(() => {
			if (scrollToMessage(messageId)) {
				pendingNavigationRef.current = null;
			}
		});
	}, [scrollToMessage]);

	/**
	 * Find message ID by line number (for hstry search results).
	 * This maps line numbers to message IDs based on message order.
	 */
	const findMessageByLineNumber = useCallback(
		(lineNumber: number): string | null => {
			// Line numbers from hstry correspond to the JSONL line in the session file.
			// Messages are typically 1-indexed in the file (after header).
			// This is a heuristic - exact mapping depends on session format.
			const messageIndex = Math.max(0, lineNumber - 2); // Account for header line
			if (messageIndex < messages.length) {
				return messages[messageIndex].id;
			}
			return null;
		},
		[messages],
	);

	return {
		scrollToMessage,
		navigateToMessage,
		findMessageByLineNumber,
	};
}

/**
 * CSS for the search highlight animation.
 * Add this to your global styles or include in the component.
 */
export const searchHighlightStyles = `
.search-highlight {
  animation: search-highlight-pulse 2s ease-out;
}

@keyframes search-highlight-pulse {
  0% { background-color: hsl(var(--primary) / 0.3); }
  50% { background-color: hsl(var(--primary) / 0.15); }
  100% { background-color: transparent; }
}
`;
