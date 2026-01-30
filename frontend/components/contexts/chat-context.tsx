"use client";

import {
	type ChatSession,
	type ProjectEntry,
	listChatHistory,
	newWorkspacePiSession,
	updateChatSession,
} from "@/lib/control-plane-client";
import {
	type OpenCodeSession,
	createSession,
	deleteSession,
	fetchSessions,
	updateSession,
} from "@/lib/opencode-client";
import { resolveReadableId } from "@/lib/session-utils";
import {
	type ReactNode,
	createContext,
	startTransition,
	useCallback,
	useContext,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useMainChatContext } from "./main-chat-context";
import { useLocale } from "./ui-context";
import { useWorkspaceContext } from "./workspace-context";

export interface ChatContextValue {
	/** Chat sessions from disk (no running opencode needed) */
	chatHistory: ChatSession[];
	/** Live opencode sessions (requires running opencode) */
	opencodeSessions: OpenCodeSession[];
	selectedChatSessionId: string;
	setSelectedChatSessionId: (id: string) => void;
	selectedChatSession: OpenCodeSession | undefined;
	/** Get the selected chat from history (may not have live opencode session) */
	selectedChatFromHistory: ChatSession | undefined;
	/** Set of chat session IDs that are currently busy (agent working) */
	busySessions: Set<string>;
	/** Mark a session as busy or idle */
	setSessionBusy: (sessionId: string, busy: boolean) => void;
	refreshChatHistory: () => Promise<void>;
	refreshOpencodeSessions: () => Promise<void>;
	/** Create a placeholder chat session for instant UI feedback. */
	createOptimisticChatSession: (workspacePath?: string) => string;
	/** Remove a placeholder chat session. */
	clearOptimisticChatSession: (sessionId: string) => void;
	createNewChat: (
		baseUrlOverride?: string,
		directoryOverride?: string,
		options?: { optimisticId?: string },
	) => Promise<OpenCodeSession | null>;
	createNewPiChat: (
		workspacePath?: string,
		options?: { optimisticId?: string },
	) => Promise<string | null>;
	deleteChatSession: (
		sessionId: string,
		baseUrlOverride?: string,
	) => Promise<boolean>;
	renameChatSession: (sessionId: string, title: string) => Promise<boolean>;
	/** Directory for the current opencode session */
	opencodeDirectory?: string;
}

// Default no-op functions for HMR resilience
const noop = () => {};
const asyncNoop = async () => null;
const asyncNoopVoid = async () => {};
const asyncNoopBool = async () => false;

const defaultChatContext: ChatContextValue = {
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
	createNewChat: asyncNoop,
	createNewPiChat: asyncNoop,
	deleteChatSession: asyncNoopBool,
	renameChatSession: asyncNoopBool,
	opencodeDirectory: undefined,
};

const ChatContext = createContext<ChatContextValue>(defaultChatContext);

export function ChatProvider({ children }: { children: ReactNode }) {
	const { locale } = useLocale();
	const { mainChatActive, setMainChatActive } = useMainChatContext();
	const { opencodeBaseUrl, selectedWorkspaceSession, projects } =
		useWorkspaceContext();

	// Chat history from disk (no running opencode needed)
	const [chatHistory, setChatHistory] = useState<ChatSession[]>([]);
	const chatHistoryRef = useRef<ChatSession[]>([]);
	const optimisticChatSessionsRef = useRef<Map<string, ChatSession>>(new Map());
	const optimisticSelectionRef = useRef<Map<string, string>>(new Map());
	// Track sessions that were just explicitly created - prevents refresh from overriding selection
	const recentlyCreatedSessionRef = useRef<string | null>(null);
	chatHistoryRef.current = chatHistory;

	// Live opencode sessions (requires running opencode instance)
	const [opencodeSessions, setOpencodeSessions] = useState<OpenCodeSession[]>(
		[],
	);

	const [selectedChatSessionId, setSelectedChatSessionIdRaw] = useState<string>(
		() => {
			if (typeof window !== "undefined") {
				const params = new URLSearchParams(window.location.search);
				const mockSession = params.get("mockSession");
				if (mockSession) {
					console.log("[Dev] Using mock session ID:", mockSession);
					return mockSession;
				}
				// Restore last used session
				try {
					return localStorage.getItem("octo:lastChatSessionId") || "";
				} catch {
					return "";
				}
			}
			return "";
		},
	);

	// Wrap setter to persist to localStorage
	const setSelectedChatSessionId = useCallback(
		(value: string | ((prev: string) => string)) => {
			setSelectedChatSessionIdRaw((prev) => {
				const newId = typeof value === "function" ? value(prev) : value;
				if (typeof window !== "undefined") {
					try {
						if (newId) {
							localStorage.setItem("octo:lastChatSessionId", newId);
						} else {
							localStorage.removeItem("octo:lastChatSessionId");
						}
					} catch {
						// Ignore localStorage errors
					}
				}
				return newId;
			});
		},
		[],
	);

	// Track which chat sessions are currently busy (agent working)
	const [busySessions, setBusySessions] = useState<Set<string>>(new Set());

	const setSessionBusy = useCallback((sessionId: string, busy: boolean) => {
		setBusySessions((prev) => {
			const next = new Set(prev);
			if (busy) {
				next.add(sessionId);
			} else {
				next.delete(sessionId);
			}
			return next;
		});
	}, []);

	const selectedChatSession = useMemo(() => {
		return opencodeSessions.find((s) => s.id === selectedChatSessionId);
	}, [opencodeSessions, selectedChatSessionId]);

	const selectedChatFromHistory = useMemo(() => {
		return chatHistory.find((s) => s.id === selectedChatSessionId);
	}, [chatHistory, selectedChatSessionId]);

	const opencodeDirectory = useMemo(() => {
		return (
			selectedChatFromHistory?.workspace_path ??
			selectedWorkspaceSession?.workspace_path
		);
	}, [selectedChatFromHistory, selectedWorkspaceSession]);

	const resolveProjectName = useCallback(
		(workspacePath: string) => {
			const normalized = workspacePath.replace(/\\/g, "/").replace(/\/+$/, "");
			const project = projects.find(
				(entry: ProjectEntry) =>
					entry.path === workspacePath || entry.path === normalized,
			);
			if (project?.name) return project.name;
			const parts = normalized.split("/").filter(Boolean);
			if (parts.length > 0) return parts[parts.length - 1];
			return locale === "de" ? "Arbeitsbereich" : "Workspace";
		},
		[projects, locale],
	);

	const createOptimisticChatSession = useCallback(
		(workspacePath?: string) => {
			const resolvedPath =
				workspacePath?.trim() || opencodeDirectory || "global";
			const now = Date.now();
			const optimisticId = `pending-${now}-${Math.random().toString(36).slice(2, 10)}`;
			const session: ChatSession = {
				id: optimisticId,
				readable_id: resolveReadableId(optimisticId, null),
				title: locale === "de" ? "Neue Sitzung" : "New Session",
				parent_id: null,
				workspace_path: resolvedPath,
				project_name: resolveProjectName(resolvedPath),
				created_at: now,
				updated_at: now,
				version: null,
				is_child: false,
				source_path: null,
			};
			optimisticChatSessionsRef.current.set(optimisticId, session);
			optimisticSelectionRef.current.set(optimisticId, selectedChatSessionId);
			setChatHistory((prev) => [session, ...prev]);
			setSelectedChatSessionId(optimisticId);
			return optimisticId;
		},
		[
			locale,
			opencodeDirectory,
			resolveProjectName,
			selectedChatSessionId,
			setSelectedChatSessionId,
		],
	);

	const clearOptimisticChatSession = useCallback(
		(sessionId: string) => {
			if (!optimisticChatSessionsRef.current.has(sessionId)) return;
			optimisticChatSessionsRef.current.delete(sessionId);
			const previousSelection = optimisticSelectionRef.current.get(sessionId);
			optimisticSelectionRef.current.delete(sessionId);
			setChatHistory((prev) =>
				prev.filter((session) => session.id !== sessionId),
			);
			if (selectedChatSessionId === sessionId) {
				setSelectedChatSessionId(previousSelection || "");
			}
		},
		[selectedChatSessionId, setSelectedChatSessionId],
	);

	const replaceOptimisticChatSession = useCallback(
		(optimisticId: string, sessionId: string) => {
			const optimistic = optimisticChatSessionsRef.current.get(optimisticId);
			if (!optimistic) return;
			optimisticChatSessionsRef.current.delete(optimisticId);
			optimisticSelectionRef.current.delete(optimisticId);

			const updated: ChatSession = {
				...optimistic,
				id: sessionId,
				readable_id: resolveReadableId(sessionId, null),
			};

			optimisticChatSessionsRef.current.set(sessionId, updated);
			setChatHistory((prev) => {
				const filtered = prev.filter(
					(session) => session.id !== optimisticId && session.id !== sessionId,
				);
				return [updated, ...filtered];
			});
		},
		[],
	);

	// Refresh chat history from disk
	const refreshChatHistory = useCallback(async () => {
		try {
			const history = await listChatHistory({ include_children: true });
			startTransition(() => {
				const optimisticSessions = Array.from(
					optimisticChatSessionsRef.current.values(),
				).filter((session) => !history.some((item) => item.id === session.id));
				setChatHistory(
					optimisticSessions.length > 0
						? [...optimisticSessions, ...history]
						: history,
				);

				if (history.length > 0 && !mainChatActive) {
					setSelectedChatSessionId((current) => {
						// Don't override selection for recently created sessions
						if (current && current === recentlyCreatedSessionRef.current) {
							return current;
						}
						if (current && opencodeSessions.some((s) => s.id === current)) {
							return current;
						}
						if (current && history.some((s) => s.id === current))
							return current;
						if (current && optimisticChatSessionsRef.current.has(current)) {
							return current;
						}
						return history[0].id;
					});
				}
			});
		} catch (err) {
			console.error("Failed to load chat history:", err);
		}
	}, [mainChatActive, opencodeSessions, setSelectedChatSessionId]);

	const refreshOpencodeSessions = useCallback(async () => {
		if (!opencodeBaseUrl) return;
		try {
			const sessions = await fetchSessions(opencodeBaseUrl, {
				directory: opencodeDirectory,
			});
			setOpencodeSessions(sessions);
			const history = chatHistoryRef.current;
			if (sessions.length > 0 && !mainChatActive) {
				const sorted = [...sessions].sort(
					(a, b) => b.time.updated - a.time.updated,
				);
				setSelectedChatSessionId((current) => {
					// Don't override selection for recently created sessions
					if (current && current === recentlyCreatedSessionRef.current) {
						return current;
					}
					if (!current) return sorted[0].id;
					if (sessions.some((s) => s.id === current)) return current;
					if (history.some((s) => s.id === current)) return current;
					return sorted[0].id;
				});
			} else {
				if (!mainChatActive) {
					setSelectedChatSessionId((current) => {
						if (current && history.some((s) => s.id === current)) {
							return current;
						}
						return current;
					});
				}
				if (history.length === 0 && !mainChatActive) {
					const created = await createSession(
						opencodeBaseUrl,
						undefined,
						undefined,
						{ directory: opencodeDirectory },
					);
					setOpencodeSessions([created]);
					setSelectedChatSessionId(created.id);
				}
			}
		} catch (err) {
			console.error("Failed to load opencode sessions:", err);
		}
	}, [
		mainChatActive,
		opencodeBaseUrl,
		opencodeDirectory,
		setSelectedChatSessionId,
	]);

	const createNewChat = useCallback(
		async (
			baseUrlOverride?: string,
			directoryOverride?: string,
			options?: { optimisticId?: string },
		): Promise<OpenCodeSession | null> => {
			const baseUrl = baseUrlOverride || opencodeBaseUrl;
			if (!baseUrl) return null;
			try {
				const directory = directoryOverride || opencodeDirectory;
				const created = await createSession(baseUrl, undefined, undefined, {
					directory,
				});
				if (options?.optimisticId) {
					clearOptimisticChatSession(options.optimisticId);
				}
				setOpencodeSessions((prev) => [created, ...prev]);
				// Mark as recently created to prevent refresh from overriding selection
				recentlyCreatedSessionRef.current = created.id;
				setSelectedChatSessionId(created.id);
				setTimeout(() => {
					refreshChatHistory();
					// Clear the recently created flag after refresh completes
					setTimeout(() => {
						if (recentlyCreatedSessionRef.current === created.id) {
							recentlyCreatedSessionRef.current = null;
						}
					}, 100);
				}, 500);
				return created;
			} catch (err) {
				if (options?.optimisticId) {
					clearOptimisticChatSession(options.optimisticId);
				}
				console.error("Failed to create new chat session:", err);
				return null;
			}
		},
		[
			opencodeBaseUrl,
			opencodeDirectory,
			refreshChatHistory,
			clearOptimisticChatSession,
			setSelectedChatSessionId,
		],
	);

	const createNewPiChat = useCallback(
		async (
			workspacePathOverride?: string,
			options?: { optimisticId?: string },
		): Promise<string | null> => {
			setMainChatActive(false);
			const resolvedPath =
				workspacePathOverride?.trim() ||
				opencodeDirectory ||
				selectedChatFromHistory?.workspace_path ||
				"global";
			try {
				const newState = await newWorkspacePiSession(resolvedPath);
				const sessionId = newState.session_id;
				if (!sessionId) {
					throw new Error("Pi session id missing");
				}
				if (options?.optimisticId) {
					replaceOptimisticChatSession(options.optimisticId, sessionId);
				}
				recentlyCreatedSessionRef.current = sessionId;
				setSelectedChatSessionId(sessionId);
				setTimeout(() => {
					refreshChatHistory();
					setTimeout(() => {
						if (recentlyCreatedSessionRef.current === sessionId) {
							recentlyCreatedSessionRef.current = null;
						}
					}, 100);
				}, 500);
				return sessionId;
			} catch (err) {
				if (options?.optimisticId) {
					clearOptimisticChatSession(options.optimisticId);
				}
				console.error("Failed to create new Pi chat session:", err);
				return null;
			}
		},
		[
			clearOptimisticChatSession,
			opencodeDirectory,
			replaceOptimisticChatSession,
			refreshChatHistory,
			selectedChatFromHistory,
			setSelectedChatSessionId,
			setMainChatActive,
		],
	);

	const deleteChatSession = useCallback(
		async (sessionId: string, baseUrlOverride?: string): Promise<boolean> => {
			const baseUrl = baseUrlOverride || opencodeBaseUrl;
			if (!baseUrl) return false;
			try {
				await deleteSession(baseUrl, sessionId, {
					directory: opencodeDirectory,
				});
				setOpencodeSessions((prev) => prev.filter((s) => s.id !== sessionId));
				setSelectedChatSessionId((current) => {
					if (current !== sessionId) return current;
					const remaining = opencodeSessions.filter((s) => s.id !== sessionId);
					return remaining.length > 0 ? remaining[0].id : "";
				});
				setTimeout(() => {
					refreshChatHistory();
				}, 500);
				return true;
			} catch (err) {
				console.error("Failed to delete chat session:", err);
				return false;
			}
		},
		[
			opencodeBaseUrl,
			opencodeDirectory,
			opencodeSessions,
			refreshChatHistory,
			setSelectedChatSessionId,
		],
	);

	const renameChatSession = useCallback(
		async (sessionId: string, title: string): Promise<boolean> => {
			try {
				if (opencodeBaseUrl) {
					try {
						const updated = await updateSession(
							opencodeBaseUrl,
							sessionId,
							{ title },
							{ directory: opencodeDirectory },
						);
						setOpencodeSessions((prev) =>
							prev.map((s) => (s.id === sessionId ? updated : s)),
						);
						setChatHistory((prev) =>
							prev.map((s) => (s.id === sessionId ? { ...s, title } : s)),
						);
						return true;
					} catch {
						// Fall through to try the history API
					}
				}

				const updated = await updateChatSession(sessionId, { title });
				setChatHistory((prev) =>
					prev.map((s) =>
						s.id === sessionId ? { ...s, title: updated.title } : s,
					),
				);
				return true;
			} catch (err) {
				console.error("Failed to rename chat session:", err);
				return false;
			}
		},
		[opencodeBaseUrl, opencodeDirectory],
	);

	// Initial load
	useEffect(() => {
		refreshChatHistory();
	}, [refreshChatHistory]);

	useEffect(() => {
		refreshOpencodeSessions();
	}, [refreshOpencodeSessions]);

	const value = useMemo(
		() => ({
			chatHistory,
			opencodeSessions,
			selectedChatSessionId,
			setSelectedChatSessionId,
			selectedChatSession,
			selectedChatFromHistory,
			busySessions,
			setSessionBusy,
			refreshChatHistory,
			refreshOpencodeSessions,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			createNewChat,
			createNewPiChat,
			deleteChatSession,
			renameChatSession,
			opencodeDirectory,
		}),
		[
			chatHistory,
			opencodeSessions,
			selectedChatSessionId,
			setSelectedChatSessionId,
			selectedChatSession,
			selectedChatFromHistory,
			busySessions,
			setSessionBusy,
			refreshChatHistory,
			refreshOpencodeSessions,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			createNewChat,
			createNewPiChat,
			deleteChatSession,
			renameChatSession,
			opencodeDirectory,
		],
	);

	return <ChatContext.Provider value={value}>{children}</ChatContext.Provider>;
}

export function useChatContext() {
	return useContext(ChatContext);
}

// Convenience hooks that match the original APIs
export function useBusySessions() {
	const { busySessions, setSessionBusy } = useChatContext();
	return { busySessions, setSessionBusy };
}

export function useChatHistory() {
	const { chatHistory, refreshChatHistory } = useChatContext();
	return { chatHistory, refreshChatHistory };
}

export function useSelectedChat() {
	const {
		selectedChatSessionId,
		setSelectedChatSessionId,
		selectedChatSession,
		selectedChatFromHistory,
	} = useChatContext();
	return {
		selectedChatSessionId,
		setSelectedChatSessionId,
		selectedChatSession,
		selectedChatFromHistory,
	};
}
