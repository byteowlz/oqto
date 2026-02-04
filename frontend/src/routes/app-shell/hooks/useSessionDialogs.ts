import type { ChatSession } from "@/lib/control-plane-client";
import { normalizeWorkspacePath } from "@/lib/session-utils";
import { useCallback, useState } from "react";

export interface SessionDialogsState {
	// Delete dialog
	deleteDialogOpen: boolean;
	setDeleteDialogOpen: (open: boolean) => void;
	targetSessionId: string;

	// Rename dialog
	renameDialogOpen: boolean;
	setRenameDialogOpen: (open: boolean) => void;
	renameInitialValue: string;

	// Delete project dialog
	deleteProjectDialogOpen: boolean;
	setDeleteProjectDialogOpen: (open: boolean) => void;
	targetProjectKey: string;
	targetProjectName: string;

	// Rename project dialog
	renameProjectDialogOpen: boolean;
	setRenameProjectDialogOpen: (open: boolean) => void;
	renameProjectInitialValue: string;

	// Handlers
	handleDeleteSession: (sessionId: string) => void;
	handleRenameSession: (sessionId: string, chatHistory: ChatSession[]) => void;
	handleConfirmDelete: (
		deleteChatSession: (sessionId: string) => Promise<boolean>,
		chatHistory: ChatSession[],
	) => Promise<void>;
	handleConfirmRename: (
		newTitle: string,
		renameChatSession: (sessionId: string, title: string) => Promise<boolean>,
	) => Promise<void>;

	handleDeleteProject: (projectKey: string, projectName: string) => void;
	handleRenameProject: (projectKey: string, currentName: string) => void;
	handleConfirmDeleteProject: (
		chatHistory: ChatSession[],
		deleteChatSession: (sessionId: string) => Promise<boolean>,
	) => Promise<void>;
	handleConfirmRenameProject: (newName: string) => Promise<void>;
}

export function useSessionDialogs(): SessionDialogsState {
	// Delete dialog state
	const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
	const [targetSessionId, setTargetSessionId] = useState<string>("");

	// Rename dialog state
	const [renameDialogOpen, setRenameDialogOpen] = useState(false);
	const [renameInitialValue, setRenameInitialValue] = useState("");

	// Delete project dialog state
	const [deleteProjectDialogOpen, setDeleteProjectDialogOpen] = useState(false);
	const [targetProjectKey, setTargetProjectKey] = useState<string>("");
	const [targetProjectName, setTargetProjectName] = useState<string>("");

	// Rename project dialog state
	const [renameProjectDialogOpen, setRenameProjectDialogOpen] = useState(false);
	const [renameProjectInitialValue, setRenameProjectInitialValue] =
		useState("");

	const handleDeleteSession = useCallback((sessionId: string) => {
		setTargetSessionId(sessionId);
		setDeleteDialogOpen(true);
	}, []);

	const handleRenameSession = useCallback(
		(sessionId: string, chatHistory: ChatSession[]) => {
			const session = chatHistory.find((s) => s.id === sessionId);
			setTargetSessionId(sessionId);
			setRenameInitialValue(session?.title || "");
			setRenameDialogOpen(true);
		},
		[],
	);

	const handleConfirmDelete = useCallback(
		async (
			deleteChatSession: (sessionId: string) => Promise<boolean>,
			chatHistory: ChatSession[],
		) => {
			if (targetSessionId) {
				await deleteChatSession(targetSessionId);
			}
			setDeleteDialogOpen(false);
			setTargetSessionId("");
		},
		[targetSessionId],
	);

	const handleConfirmRename = useCallback(
		async (
			newTitle: string,
			renameChatSession: (sessionId: string, title: string) => Promise<boolean>,
		) => {
			if (targetSessionId && newTitle.trim()) {
				await renameChatSession(targetSessionId, newTitle.trim());
			}
			setRenameDialogOpen(false);
			setTargetSessionId("");
		},
		[targetSessionId],
	);

	const handleDeleteProject = useCallback(
		(projectKey: string, projectName: string) => {
			setTargetProjectKey(projectKey);
			setTargetProjectName(projectName);
			setDeleteProjectDialogOpen(true);
		},
		[],
	);

	const handleRenameProject = useCallback(
		(projectKey: string, currentName: string) => {
			setTargetProjectKey(projectKey);
			setTargetProjectName(currentName);
			setRenameProjectInitialValue(currentName);
			setRenameProjectDialogOpen(true);
		},
		[],
	);

	const handleConfirmDeleteProject = useCallback(
		async (
			chatHistory: ChatSession[],
			deleteChatSession: (sessionId: string) => Promise<boolean>,
		) => {
			if (targetProjectKey) {
				const sessionsToDelete = chatHistory.filter((s) => {
					const normalizedPath = normalizeWorkspacePath(s.workspace_path);
					if (!normalizedPath) return false;
					const key =
						normalizedPath.split("/").filter(Boolean).pop() || normalizedPath;
					return key === targetProjectKey;
				});

				for (const session of sessionsToDelete) {
					await deleteChatSession(session.id);
				}
			}
			setDeleteProjectDialogOpen(false);
			setTargetProjectKey("");
			setTargetProjectName("");
		},
		[targetProjectKey],
	);

	const handleConfirmRenameProject = useCallback(
		async (newName: string) => {
			if (targetProjectKey && newName.trim()) {
				// TODO: Implement project rename via backend API
				console.log(
					"[handleConfirmRenameProject] Would rename project:",
					targetProjectKey,
					"to:",
					newName.trim(),
				);
			}
			setRenameProjectDialogOpen(false);
			setTargetProjectKey("");
			setTargetProjectName("");
		},
		[targetProjectKey],
	);

	return {
		deleteDialogOpen,
		setDeleteDialogOpen,
		targetSessionId,
		renameDialogOpen,
		setRenameDialogOpen,
		renameInitialValue,
		deleteProjectDialogOpen,
		setDeleteProjectDialogOpen,
		targetProjectKey,
		targetProjectName,
		renameProjectDialogOpen,
		setRenameProjectDialogOpen,
		renameProjectInitialValue,
		handleDeleteSession,
		handleRenameSession,
		handleConfirmDelete,
		handleConfirmRename,
		handleDeleteProject,
		handleRenameProject,
		handleConfirmDeleteProject,
		handleConfirmRenameProject,
	};
}
