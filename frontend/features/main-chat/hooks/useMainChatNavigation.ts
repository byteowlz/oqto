"use client";

import { getMainChatAssistant } from "@/features/main-chat/api";
import { useCallback } from "react";

type MainChatNavigationOptions = {
	setMainChatAssistantName: (name: string | null) => void;
	setMainChatActive: (active: boolean) => void;
	setMainChatCurrentSessionId: (sessionId: string | null) => void;
	setSelectedChatSessionId: (sessionId: string) => void;
	setActiveAppId: (appId: string) => void;
	setMobileMenuOpen: (open: boolean) => void;
	setMainChatWorkspacePath: (path: string | null) => void;
};

export function useMainChatNavigation({
	setMainChatAssistantName,
	setMainChatActive,
	setMainChatCurrentSessionId,
	setSelectedChatSessionId,
	setActiveAppId,
	setMobileMenuOpen,
	setMainChatWorkspacePath,
}: MainChatNavigationOptions) {
	const hydrateWorkspacePath = useCallback(
		(assistantName: string) => {
			getMainChatAssistant(assistantName)
				.then((info) => setMainChatWorkspacePath(info.path))
				.catch((err) => {
					console.error("Failed to load Main Chat assistant info:", err);
					setMainChatWorkspacePath(null);
				});
		},
		[setMainChatWorkspacePath],
	);

	const handleMainChatSelect = useCallback(
		async (assistantName: string, sessionId: string | null) => {
			setMainChatAssistantName(assistantName);
			setMainChatActive(true);
			setMainChatCurrentSessionId(sessionId);
			setSelectedChatSessionId("");
			setActiveAppId("sessions");
			setMobileMenuOpen(false);
			hydrateWorkspacePath(assistantName);
		},
		[
			hydrateWorkspacePath,
			setActiveAppId,
			setMainChatActive,
			setMainChatAssistantName,
			setMainChatCurrentSessionId,
			setMobileMenuOpen,
			setSelectedChatSessionId,
		],
	);

	const handleMainChatSessionSelect = useCallback(
		async (assistantName: string, sessionId: string) => {
			setMainChatAssistantName(assistantName);
			setMainChatActive(true);
			setMainChatCurrentSessionId(sessionId);
			setSelectedChatSessionId("");
			setActiveAppId("sessions");
			setMobileMenuOpen(false);
			hydrateWorkspacePath(assistantName);
		},
		[
			hydrateWorkspacePath,
			setActiveAppId,
			setMainChatActive,
			setMainChatAssistantName,
			setMainChatCurrentSessionId,
			setMobileMenuOpen,
			setSelectedChatSessionId,
		],
	);

	const handleMainChatNewSession = useCallback(
		(assistantName: string) => {
			setMainChatAssistantName(assistantName);
			setMainChatActive(true);
			setMainChatCurrentSessionId("/new");
			setSelectedChatSessionId("");
			setActiveAppId("sessions");
			setMobileMenuOpen(false);
			hydrateWorkspacePath(assistantName);
		},
		[
			hydrateWorkspacePath,
			setActiveAppId,
			setMainChatActive,
			setMainChatAssistantName,
			setMainChatCurrentSessionId,
			setMobileMenuOpen,
			setSelectedChatSessionId,
		],
	);

	return {
		handleMainChatSelect,
		handleMainChatSessionSelect,
		handleMainChatNewSession,
	};
}
