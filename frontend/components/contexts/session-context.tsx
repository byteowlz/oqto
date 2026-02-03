"use client";

/**
 * Session Context - Composition Layer
 *
 * This file composes the focused session contexts and provides a unified API
 * for backward compatibility. New code should prefer importing from the
 * individual contexts directly:
 *
 * - workspace-context.tsx: Workspace session management
 * - chat-context.tsx: Chat session management
 * - default-chat-context.tsx: Default chat Pi state
 */

import type {
	ChatSession,
	ProjectEntry,
	WorkspaceSession,
} from "@/lib/control-plane-client";
import type { OpenCodeSession } from "@/lib/opencode-client";
import {
	type Dispatch,
	type ReactNode,
	type SetStateAction,
	createContext,
	useContext,
	useMemo,
} from "react";

// Re-export individual context providers and hooks
export {
	DefaultChatProvider,
	useDefaultChatContext,
	useDefaultChat,
	type DefaultChatContextValue,
} from "./default-chat-context";

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

// Import for internal use
import { useChatContext } from "./chat-context";
import { useDefaultChatContext } from "./default-chat-context";
import { useWorkspaceContext } from "./workspace-context";

/**
 * Combined SessionContextValue for backward compatibility.
 * New code should prefer the focused hooks from individual contexts.
 */
export interface SessionContextValue {
	// Workspace state
	workspaceSessions: WorkspaceSession[];
	selectedWorkspaceSessionId: string;
	setSelectedWorkspaceSessionId: (id: string) => void;
	selectedWorkspaceSession: WorkspaceSession | undefined;
	opencodeBaseUrl: string;
	opencodeDirectory?: string;
	projects: ProjectEntry[];
	startProjectSession: (
		projectPath: string,
	) => Promise<WorkspaceSession | null>;
	projectDefaultAgents: Record<string, string>;
	setProjectDefaultAgents: Dispatch<SetStateAction<Record<string, string>>>;
	refreshWorkspaceSessions: () => Promise<void>;
	ensureOpencodeRunning: (workspacePath?: string) => Promise<string | null>;
	stopWorkspaceSession: (sessionId: string) => Promise<boolean>;
	deleteWorkspaceSession: (sessionId: string) => Promise<boolean>;
	upgradeWorkspaceSession: (sessionId: string) => Promise<boolean>;

	// Chat state
	chatHistory: ChatSession[];
	opencodeSessions: OpenCodeSession[];
	selectedChatSessionId: string;
	setSelectedChatSessionId: (id: string) => void;
	selectedChatSession: OpenCodeSession | undefined;
	selectedChatFromHistory: ChatSession | undefined;
	busySessions: Set<string>;
	setSessionBusy: (sessionId: string, busy: boolean) => void;
	refreshChatHistory: () => Promise<void>;
	refreshOpencodeSessions: () => Promise<void>;
	createOptimisticChatSession: (workspacePath?: string) => string;
	clearOptimisticChatSession: (sessionId: string) => void;
	replaceOptimisticChatSession: (
		optimisticId: string,
		sessionId: string,
	) => void;
	createNewChat: (
		baseUrlOverride?: string,
		directoryOverride?: string,
		options?: { optimisticId?: string },
	) => Promise<OpenCodeSession | null>;
	createNewChat: (
		workspacePath?: string,
		options?: { optimisticId?: string },
	) => Promise<string | null>;
	deleteChatSession: (
		sessionId: string,
		baseUrlOverride?: string,
	) => Promise<boolean>;
	renameChatSession: (sessionId: string, title: string) => Promise<boolean>;

	// Default Chat state
	defaultChatActive: boolean;
	setDefaultChatActive: (active: boolean) => void;
	defaultChatAssistantName: string | null;
	setDefaultChatAssistantName: (name: string | null) => void;
	defaultChatCurrentSessionId: string | null;
	setDefaultChatCurrentSessionId: (id: string | null) => void;
	defaultChatWorkspacePath: string | null;
	setDefaultChatWorkspacePath: (path: string | null) => void;
	sessionActivityTrigger: number;
	notifySessionActivity: () => void;
	scrollToMessageId: string | null;
	setScrollToMessageId: (id: string | null) => void;
}

// Default no-op functions for HMR resilience
const noop = () => {};
const asyncNoop = async () => null;
const asyncNoopVoid = async () => {};
const asyncNoopBool = async () => false;

const defaultSessionContext: SessionContextValue = {
	// Workspace defaults
	workspaceSessions: [],
	selectedWorkspaceSessionId: "",
	setSelectedWorkspaceSessionId: noop,
	selectedWorkspaceSession: undefined,
	opencodeBaseUrl: "",
	opencodeDirectory: undefined,
	projects: [],
	startProjectSession: asyncNoop,
	projectDefaultAgents: {},
	setProjectDefaultAgents: noop,
	refreshWorkspaceSessions: asyncNoopVoid,
	ensureOpencodeRunning: asyncNoop,
	stopWorkspaceSession: asyncNoopBool,
	deleteWorkspaceSession: asyncNoopBool,
	upgradeWorkspaceSession: asyncNoopBool,

	// Chat defaults
	chatHistory: [],
	opencodeSessions: [],
	selectedChatSessionId: "",
	setSelectedChatSessionId: noop,
	selectedChatSession: undefined,
	selectedChatFromHistory: undefined,
	busySessions: new Set(),
	setSessionBusy: noop,
	refreshChatHistory: asyncNoopVoid,
	refreshOpencodeSessions: asyncNoopVoid,
	createOptimisticChatSession: () => "",
	clearOptimisticChatSession: noop,
	replaceOptimisticChatSession: noop,
	createNewChat: asyncNoop,
	createNewChat: asyncNoop,
	deleteChatSession: asyncNoopBool,
	renameChatSession: asyncNoopBool,

	// Default Chat defaults
	defaultChatActive: false,
	setDefaultChatActive: noop,
	defaultChatAssistantName: null,
	setDefaultChatAssistantName: noop,
	defaultChatCurrentSessionId: null,
	setDefaultChatCurrentSessionId: noop,
	defaultChatWorkspacePath: null,
	setDefaultChatWorkspacePath: noop,
	sessionActivityTrigger: 0,
	notifySessionActivity: noop,
	scrollToMessageId: null,
	setScrollToMessageId: noop,
};

const SessionContext = createContext<SessionContextValue>(
	defaultSessionContext,
);

import { ChatProvider } from "./chat-context";
// Import providers for nesting
import { DefaultChatProvider } from "./default-chat-context";
import { WorkspaceProvider } from "./workspace-context";

/**
 * SessionProvider - Composition provider that nests all session contexts.
 *
 * Provides the combined SessionContextValue for backward compatibility.
 * The individual providers are nested in order:
 * 1. DefaultChatProvider (no dependencies)
 * 2. WorkspaceProvider (no dependencies)
 * 3. ChatProvider (depends on DefaultChatContext, WorkspaceContext)
 * 4. SessionContextComposer (combines all for backward compatibility)
 */
export function SessionProvider({ children }: { children: ReactNode }) {
	return (
		<DefaultChatProvider>
			<WorkspaceProvider>
				<ChatProvider>
					<SessionContextComposer>{children}</SessionContextComposer>
				</ChatProvider>
			</WorkspaceProvider>
		</DefaultChatProvider>
	);
}

/**
 * Internal component that composes all context values.
 * Must be rendered inside all individual providers.
 */
function SessionContextComposer({ children }: { children: ReactNode }) {
	const workspace = useWorkspaceContext();
	const chat = useChatContext();
	const defaultChat = useDefaultChatContext();

	const value = useMemo<SessionContextValue>(
		() => ({
			// Workspace state
			workspaceSessions: workspace.workspaceSessions,
			selectedWorkspaceSessionId: workspace.selectedWorkspaceSessionId,
			setSelectedWorkspaceSessionId: workspace.setSelectedWorkspaceSessionId,
			selectedWorkspaceSession: workspace.selectedWorkspaceSession,
			opencodeBaseUrl: workspace.opencodeBaseUrl,
			opencodeDirectory: chat.opencodeDirectory,
			projects: workspace.projects,
			startProjectSession: workspace.startProjectSession,
			projectDefaultAgents: workspace.projectDefaultAgents,
			setProjectDefaultAgents: workspace.setProjectDefaultAgents,
			refreshWorkspaceSessions: workspace.refreshWorkspaceSessions,
			ensureOpencodeRunning: workspace.ensureOpencodeRunning,
			stopWorkspaceSession: workspace.stopWorkspaceSession,
			deleteWorkspaceSession: workspace.deleteWorkspaceSession,
			upgradeWorkspaceSession: workspace.upgradeWorkspaceSession,

			// Chat state
			chatHistory: chat.chatHistory,
			opencodeSessions: chat.opencodeSessions,
			selectedChatSessionId: chat.selectedChatSessionId,
			setSelectedChatSessionId: chat.setSelectedChatSessionId,
			selectedChatSession: chat.selectedChatSession,
			selectedChatFromHistory: chat.selectedChatFromHistory,
			busySessions: chat.busySessions,
			setSessionBusy: chat.setSessionBusy,
			refreshChatHistory: chat.refreshChatHistory,
			refreshOpencodeSessions: chat.refreshOpencodeSessions,
			createOptimisticChatSession: chat.createOptimisticChatSession,
			clearOptimisticChatSession: chat.clearOptimisticChatSession,
			replaceOptimisticChatSession: chat.replaceOptimisticChatSession,
			createNewChat: chat.createNewChat,
			createNewChat: chat.createNewChat,
			deleteChatSession: chat.deleteChatSession,
			renameChatSession: chat.renameChatSession,

			// Default Chat state
			defaultChatActive: defaultChat.defaultChatActive,
			setDefaultChatActive: defaultChat.setDefaultChatActive,
			defaultChatAssistantName: defaultChat.defaultChatAssistantName,
			setDefaultChatAssistantName: defaultChat.setDefaultChatAssistantName,
			defaultChatCurrentSessionId: defaultChat.defaultChatCurrentSessionId,
			setDefaultChatCurrentSessionId: defaultChat.setDefaultChatCurrentSessionId,
			defaultChatWorkspacePath: defaultChat.defaultChatWorkspacePath,
			setDefaultChatWorkspacePath: defaultChat.setDefaultChatWorkspacePath,
			sessionActivityTrigger: defaultChat.sessionActivityTrigger,
			notifySessionActivity: defaultChat.notifySessionActivity,
			scrollToMessageId: defaultChat.scrollToMessageId,
			setScrollToMessageId: defaultChat.setScrollToMessageId,
		}),
		[workspace, chat, defaultChat],
	);

	return (
		<SessionContext.Provider value={value}>{children}</SessionContext.Provider>
	);
}

/**
 * useSessionContext - Access the combined session context.
 *
 * For better performance, prefer using the focused hooks:
 * - useWorkspaceContext() / useWorkspaceSessions()
 * - useChatContext() / useChatHistory() / useSelectedChat() / useBusySessions()
 * - useDefaultChatContext() / useDefaultChat()
 */
export function useSessionContext() {
	return useContext(SessionContext);
}
