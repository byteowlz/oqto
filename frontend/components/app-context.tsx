"use client";

/**
 * AppContext - Split into focused contexts for better performance
 *
 * Use the specialized hooks for optimal performance:
 * - useLocale() - for locale/i18n only
 * - useActiveApp() - for app navigation only
 * - useBusySessions() - for busy state only
 * - useChatHistory() - for chat history only
 * - useSelectedChat() - for selected chat only
 * - useWorkspaceSessions() - for workspace sessions only
 * - useMainChat() - for main chat state only
 * - useSessionContext() - for all session-related state
 * - useUIContext() - for all UI-related state
 */

import type { ReactNode } from "react";
import {
	SessionProvider,
	UIProvider,
} from "./contexts";

/**
 * AppProvider - Composes UIProvider and SessionProvider
 *
 * The split contexts ensure that:
 * - UI changes (locale, theme) don't re-render session components
 * - Session changes (busySessions) don't re-render UI components
 */
export function AppProvider({ children }: { children: ReactNode }) {
	return (
		<UIProvider>
			<SessionProvider>{children}</SessionProvider>
		</UIProvider>
	);
}

// Re-export all hooks for convenience
export {
	// UI hooks
	useUIContext,
	useLocale,
	useActiveApp,
	// Session hooks
	useSessionContext,
	useBusySessions,
	useChatHistory,
	useSelectedChat,
	useWorkspaceSessions,
	useMainChat,
} from "./contexts";
