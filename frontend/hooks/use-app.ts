"use client";

import {
	useSessionContext,
	useUIContext,
} from "@/components/contexts";
import { useMemo } from "react";

/**
 * Combined useApp hook - merges UI and Session contexts
 *
 * NOTE: This hook will cause re-renders on ANY context change.
 * For better performance, use the specialized hooks:
 *
 * - useLocale() - for locale/i18n
 * - useActiveApp() - for app navigation
 * - useBusySessions() - for busy state
 * - useChatHistory() - for chat history
 * - useSelectedChat() - for selected chat
 * - useWorkspaceSessions() - for workspace sessions
 * - useMainChat() - for main chat state
 */
export function useApp() {
	const ui = useUIContext();
	const session = useSessionContext();

	return useMemo(
		() => ({
			// UI context values
			apps: ui.apps,
			activeAppId: ui.activeAppId,
			setActiveAppId: ui.setActiveAppId,
			activeApp: ui.activeApp,
			locale: ui.locale,
			setLocale: ui.setLocale,
			resolveText: ui.resolveText,
			// Session context values
			workspaceSessions: session.workspaceSessions,
			selectedWorkspaceSessionId: session.selectedWorkspaceSessionId,
			setSelectedWorkspaceSessionId: session.setSelectedWorkspaceSessionId,
			selectedWorkspaceSession: session.selectedWorkspaceSession,
			opencodeBaseUrl: session.opencodeBaseUrl,
			opencodeDirectory: session.opencodeDirectory,
			chatHistory: session.chatHistory,
			opencodeSessions: session.opencodeSessions,
			selectedChatSessionId: session.selectedChatSessionId,
			setSelectedChatSessionId: session.setSelectedChatSessionId,
			selectedChatSession: session.selectedChatSession,
			selectedChatFromHistory: session.selectedChatFromHistory,
			busySessions: session.busySessions,
			setSessionBusy: session.setSessionBusy,
			refreshWorkspaceSessions: session.refreshWorkspaceSessions,
			refreshChatHistory: session.refreshChatHistory,
			refreshOpencodeSessions: session.refreshOpencodeSessions,
			createOptimisticChatSession: session.createOptimisticChatSession,
			clearOptimisticChatSession: session.clearOptimisticChatSession,
			ensureOpencodeRunning: session.ensureOpencodeRunning,
			createNewChat: session.createNewChat,
			createNewChatWithPersona: session.createNewChatWithPersona,
			deleteChatSession: session.deleteChatSession,
			renameChatSession: session.renameChatSession,
			stopWorkspaceSession: session.stopWorkspaceSession,
			deleteWorkspaceSession: session.deleteWorkspaceSession,
			upgradeWorkspaceSession: session.upgradeWorkspaceSession,
			projects: session.projects,
			startProjectSession: session.startProjectSession,
			projectDefaultAgents: session.projectDefaultAgents,
			setProjectDefaultAgents: session.setProjectDefaultAgents,
			mainChatActive: session.mainChatActive,
			setMainChatActive: session.setMainChatActive,
			mainChatAssistantName: session.mainChatAssistantName,
			setMainChatAssistantName: session.setMainChatAssistantName,
			mainChatCurrentSessionId: session.mainChatCurrentSessionId,
			setMainChatCurrentSessionId: session.setMainChatCurrentSessionId,
			mainChatWorkspacePath: session.mainChatWorkspacePath,
			setMainChatWorkspacePath: session.setMainChatWorkspacePath,
			mainChatNewSessionTrigger: session.mainChatNewSessionTrigger,
			requestNewMainChatSession: session.requestNewMainChatSession,
			scrollToMessageId: session.scrollToMessageId,
			setScrollToMessageId: session.setScrollToMessageId,
		}),
		[ui, session],
	);
}

// Re-export specialized hooks for convenience
export {
	useLocale,
	useActiveApp,
	useBusySessions,
	useChatHistory,
	useSelectedChat,
	useWorkspaceSessions,
	useMainChat,
} from "@/components/contexts";
