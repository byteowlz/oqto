"use client";

/**
 * Session Context - Composition Layer
 *
 * Composes workspace + chat contexts into a single surface for legacy callers.
 * Default chat and legacy OpenCode state has been removed.
 */

import type {
	ChatSession,
	ProjectEntry,
	WorkspaceSession,
} from "@/lib/control-plane-client";
import {
	type Dispatch,
	type ReactNode,
	type SetStateAction,
	createContext,
	useContext,
	useMemo,
} from "react";

export {
	WorkspaceProvider,
	useWorkspaceContext,
	useWorkspaceSessions,
	type WorkspaceContextValue,
} from "./workspace-context";

export {
	ChatProvider,
	useChatContext,
	useBusySessions,
	useChatHistory,
	useSelectedChat,
	type ChatContextValue,
} from "./chat-context";

import { useChatContext } from "./chat-context";
import { ChatProvider } from "./chat-context";
import { useWorkspaceContext } from "./workspace-context";
import { WorkspaceProvider } from "./workspace-context";

export interface SessionContextValue {
	// Workspace state
	workspaceSessions: WorkspaceSession[];
	selectedWorkspaceSessionId: string;
	setSelectedWorkspaceSessionId: (id: string) => void;
	selectedWorkspaceSession: WorkspaceSession | undefined;
	selectedWorkspaceOverviewPath: string | null;
	setSelectedWorkspaceOverviewPath: (path: string | null) => void;
	projects: ProjectEntry[];
	startProjectSession: (
		projectPath: string,
	) => Promise<WorkspaceSession | null>;
	projectDefaultAgents: Record<string, string>;
	setProjectDefaultAgents: Dispatch<SetStateAction<Record<string, string>>>;
	refreshWorkspaceSessions: () => Promise<void>;
	ensureWorkspaceRunning: (
		workspacePath?: string,
	) => Promise<WorkspaceSession | null>;
	stopWorkspaceSession: (sessionId: string) => Promise<boolean>;
	deleteWorkspaceSession: (sessionId: string) => Promise<boolean>;
	upgradeWorkspaceSession: (sessionId: string) => Promise<boolean>;

	// Chat state
	chatHistory: ChatSession[];
	selectedChatSessionId: string | null;
	setSelectedChatSessionId: (id: string | null) => void;
	selectedChatFromHistory: ChatSession | undefined;
	busySessions: Set<string>;
	setSessionBusy: (sessionId: string, busy: boolean) => void;
	runnerSessions: Array<{
		session_id: string;
		state: string;
		cwd: string;
		provider?: string;
		model?: string;
		last_activity: number;
		subscriber_count: number;
	}>;
	runnerSessionCount: number;
	refreshChatHistory: () => Promise<void>;
	createOptimisticChatSession: (
		sessionId: string,
		workspacePath?: string,
	) => string;
	clearOptimisticChatSession: (sessionId: string) => void;
	replaceOptimisticChatSession: (
		optimisticId: string,
		sessionId: string,
	) => void;
	createNewChat: (workspacePath?: string) => Promise<string | null>;
	deleteChatSession: (sessionId: string) => Promise<boolean>;
	renameChatSession: (sessionId: string, title: string) => Promise<boolean>;
	getSessionWorkspacePath: (sessionId: string | null) => string | null;
}

const noop = () => {};
const asyncNoop = async () => null;
const asyncNoopBool = async () => false;
const asyncNoopVoid = async () => {};

const defaultSessionContext: SessionContextValue = {
	workspaceSessions: [],
	selectedWorkspaceSessionId: "",
	setSelectedWorkspaceSessionId: noop,
	selectedWorkspaceSession: undefined,
	selectedWorkspaceOverviewPath: null,
	setSelectedWorkspaceOverviewPath: noop,
	projects: [],
	startProjectSession: asyncNoop,
	projectDefaultAgents: {},
	setProjectDefaultAgents: noop,
	refreshWorkspaceSessions: asyncNoopVoid,
	ensureWorkspaceRunning: asyncNoop,
	stopWorkspaceSession: asyncNoopBool,
	deleteWorkspaceSession: asyncNoopBool,
	upgradeWorkspaceSession: asyncNoopBool,

	chatHistory: [],
	selectedChatSessionId: null,
	setSelectedChatSessionId: noop,
	selectedChatFromHistory: undefined,
	busySessions: new Set(),
	setSessionBusy: noop,
	runnerSessions: [],
	runnerSessionCount: 0,
	refreshChatHistory: asyncNoopVoid,
	createOptimisticChatSession: (_sessionId?: string) => "",
	clearOptimisticChatSession: noop,
	replaceOptimisticChatSession: noop,
	createNewChat: asyncNoop,
	deleteChatSession: asyncNoopBool,
	renameChatSession: asyncNoopBool,
	getSessionWorkspacePath: () => null,
};

const SessionContext = createContext<SessionContextValue>(
	defaultSessionContext,
);

export function SessionProvider({ children }: { children: ReactNode }) {
	return (
		<WorkspaceProvider>
			<ChatProvider>
				<SessionContextComposer>{children}</SessionContextComposer>
			</ChatProvider>
		</WorkspaceProvider>
	);
}

function SessionContextComposer({ children }: { children: ReactNode }) {
	const workspace = useWorkspaceContext();
	const chat = useChatContext();

	const value = useMemo<SessionContextValue>(
		() => ({
			workspaceSessions: workspace.workspaceSessions,
			selectedWorkspaceSessionId: workspace.selectedWorkspaceSessionId,
			setSelectedWorkspaceSessionId: workspace.setSelectedWorkspaceSessionId,
			selectedWorkspaceSession: workspace.selectedWorkspaceSession,
			selectedWorkspaceOverviewPath: workspace.selectedWorkspaceOverviewPath,
			setSelectedWorkspaceOverviewPath: workspace.setSelectedWorkspaceOverviewPath,
			projects: workspace.projects,
			startProjectSession: workspace.startProjectSession,
			projectDefaultAgents: workspace.projectDefaultAgents,
			setProjectDefaultAgents: workspace.setProjectDefaultAgents,
			refreshWorkspaceSessions: workspace.refreshWorkspaceSessions,
			ensureWorkspaceRunning: workspace.ensureWorkspaceRunning,
			stopWorkspaceSession: workspace.stopWorkspaceSession,
			deleteWorkspaceSession: workspace.deleteWorkspaceSession,
			upgradeWorkspaceSession: workspace.upgradeWorkspaceSession,

			chatHistory: chat.chatHistory,
			selectedChatSessionId: chat.selectedChatSessionId,
			setSelectedChatSessionId: chat.setSelectedChatSessionId,
			selectedChatFromHistory: chat.selectedChatFromHistory,
			busySessions: chat.busySessions,
			setSessionBusy: chat.setSessionBusy,
			runnerSessions: chat.runnerSessions,
			runnerSessionCount: chat.runnerSessionCount,
			refreshChatHistory: chat.refreshChatHistory,
			createOptimisticChatSession: chat.createOptimisticChatSession,
			clearOptimisticChatSession: chat.clearOptimisticChatSession,
			replaceOptimisticChatSession: chat.replaceOptimisticChatSession,
			createNewChat: chat.createNewChat,
			deleteChatSession: chat.deleteChatSession,
			renameChatSession: chat.renameChatSession,
			getSessionWorkspacePath: chat.getSessionWorkspacePath,
		}),
		[workspace, chat],
	);

	return (
		<SessionContext.Provider value={value}>{children}</SessionContext.Provider>
	);
}

export function useSessionContext() {
	return useContext(SessionContext);
}
