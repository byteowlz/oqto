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
 * - useSessionContext() - for all session-related state
 * - useUIContext() - for all UI-related state
 * - useOnboarding() - for onboarding state and progressive unlock
 */

import type { ReactNode } from "react";
import { OnboardingProvider, SessionProvider, UIProvider } from "./contexts";

/**
 * AppProvider - Composes UIProvider, SessionProvider, and OnboardingProvider
 *
 * The split contexts ensure that:
 * - UI changes (locale, theme) don't re-render session components
 * - Session changes (busySessions) don't re-render UI components
 * - Onboarding state is available throughout the app
 */
export function AppProvider({ children }: { children: ReactNode }) {
	return (
		<UIProvider>
			<SessionProvider>
				<OnboardingProvider>{children}</OnboardingProvider>
			</SessionProvider>
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
	// Onboarding hooks
	useOnboarding,
	useIsUnlocked,
	useNeedsOnboarding,
} from "./contexts";
