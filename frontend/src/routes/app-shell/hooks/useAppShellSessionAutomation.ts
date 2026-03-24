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
	const autoCreatedRef = useRef(false);

	// useeffect-guardrail: allow - one-time session automation during initial shell state
	useEffect(() => {
		if (!autoExpandedRef.current && selectedChatSessionId) {
			const session = chatHistory.find(
				(item) => item.id === selectedChatSessionId,
			);
			if (session) {
				const key = projectKeyForSession(session);
				if (key && !expandedProjects.has(key)) {
					toggleProjectExpanded(key);
				}
				autoExpandedRef.current = true;
			}
		}

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
		chatHistory,
		createNewChat,
		expandedProjects,
		projectKeyForSession,
		selectedChatSessionId,
		toggleProjectExpanded,
		workspaceDirectories,
	]);
}
