import type { ChatSession } from "@/lib/control-plane-client";
import { useEffect, useRef } from "react";
import type { WorkspaceDirectory } from "./useProjectActions";

interface UseAppShellSessionAutomationInput {
	selectedChatSessionId: string | null;
	chatHistory: ChatSession[];
	projectKeyForSession: (session: ChatSession) => string;
	expandedProjects: Set<string>;
	toggleProjectExpanded: (projectKey: string) => void;
	workspaceDirectories: WorkspaceDirectory[];
	createNewChat: (
		workspacePath?: string,
		sharedWorkspaceId?: string,
	) => Promise<void>;
}

export function useAppShellSessionAutomation({
	selectedChatSessionId,
	chatHistory,
	projectKeyForSession,
	expandedProjects,
	toggleProjectExpanded,
	workspaceDirectories,
	createNewChat,
}: UseAppShellSessionAutomationInput): void {
	const autoExpandedRef = useRef(false);
	// useeffect-guardrail: allow - one-time auto expand based on initial selected session
	useEffect(() => {
		if (autoExpandedRef.current) return;
		if (!selectedChatSessionId) return;
		const session = chatHistory.find(
			(item) => item.id === selectedChatSessionId,
		);
		if (!session) return;
		const key = projectKeyForSession(session);
		if (key && !expandedProjects.has(key)) {
			toggleProjectExpanded(key);
		}
		autoExpandedRef.current = true;
	}, [
		chatHistory,
		expandedProjects,
		projectKeyForSession,
		selectedChatSessionId,
		toggleProjectExpanded,
	]);

	const autoCreatedRef = useRef(false);
	// useeffect-guardrail: allow - one-time bootstrap chat creation for empty workspaces
	useEffect(() => {
		if (autoCreatedRef.current) return;
		if (chatHistory.length > 0) return;
		if (selectedChatSessionId) return;
		if (workspaceDirectories.length === 0) return;
		autoCreatedRef.current = true;

		const defaultDirectory = workspaceDirectories[0];
		if (defaultDirectory?.path) {
			void createNewChat(defaultDirectory.path);
		}
	}, [
		chatHistory.length,
		createNewChat,
		selectedChatSessionId,
		workspaceDirectories,
	]);
}
