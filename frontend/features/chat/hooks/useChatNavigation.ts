"use client";

import { getDefaultChatAssistant } from "@/features/chat/api";
import { useCallback } from "react";

type ChatNavigationOptions = {
	setDefaultChatAssistantName: (name: string | null) => void;
	setDefaultChatActive: (active: boolean) => void;
	setDefaultChatCurrentSessionId: (sessionId: string | null) => void;
	setSelectedChatSessionId: (sessionId: string | null) => void;
	setActiveAppId: (appId: string) => void;
	setMobileMenuOpen: (open: boolean) => void;
	setDefaultChatWorkspacePath: (path: string | null) => void;
};

export function useChatNavigation({
	setDefaultChatAssistantName,
	setDefaultChatActive,
	setDefaultChatCurrentSessionId,
	setSelectedChatSessionId,
	setActiveAppId,
	setMobileMenuOpen,
	setDefaultChatWorkspacePath,
}: ChatNavigationOptions) {
	const hydrateWorkspacePath = useCallback(
		(assistantName: string) => {
			getDefaultChatAssistant(assistantName)
				.then((info) => setDefaultChatWorkspacePath(info.path))
				.catch((err) => {
					console.error("Failed to load Default Chat assistant info:", err);
					setDefaultChatWorkspacePath(null);
				});
		},
		[setDefaultChatWorkspacePath],
	);

	const handleDefaultChatSelect = useCallback(
		async (assistantName: string, sessionId: string | null) => {
			setDefaultChatAssistantName(assistantName);
			setDefaultChatActive(true);
			setDefaultChatCurrentSessionId(sessionId);
			setSelectedChatSessionId(null);
			setActiveAppId("sessions");
			setMobileMenuOpen(false);
			hydrateWorkspacePath(assistantName);
		},
		[
			hydrateWorkspacePath,
			setActiveAppId,
			setDefaultChatActive,
			setDefaultChatAssistantName,
			setDefaultChatCurrentSessionId,
			setMobileMenuOpen,
			setSelectedChatSessionId,
		],
	);

	const handleDefaultChatSessionSelect = useCallback(
		async (assistantName: string, sessionId: string) => {
			setDefaultChatAssistantName(assistantName);
			setDefaultChatActive(true);
			setDefaultChatCurrentSessionId(sessionId);
			setSelectedChatSessionId(null);
			setActiveAppId("sessions");
			setMobileMenuOpen(false);
			hydrateWorkspacePath(assistantName);
		},
		[
			hydrateWorkspacePath,
			setActiveAppId,
			setDefaultChatActive,
			setDefaultChatAssistantName,
			setDefaultChatCurrentSessionId,
			setMobileMenuOpen,
			setSelectedChatSessionId,
		],
	);

	const handleDefaultChatNewSession = useCallback(
		(assistantName: string) => {
			setDefaultChatAssistantName(assistantName);
			setDefaultChatActive(true);
			// Clear current session to indicate new session is being created
			setDefaultChatCurrentSessionId(null);
			setSelectedChatSessionId(null);
			setActiveAppId("sessions");
			setMobileMenuOpen(false);
			hydrateWorkspacePath(assistantName);
		},
		[
			hydrateWorkspacePath,
			setActiveAppId,
			setDefaultChatActive,
			setDefaultChatAssistantName,
			setDefaultChatCurrentSessionId,
			setMobileMenuOpen,
			setSelectedChatSessionId,
		],
	);

	return {
		handleDefaultChatSelect,
		handleDefaultChatSessionSelect,
		handleDefaultChatNewSession,
	};
}
