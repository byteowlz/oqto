"use client";

import {
	type ChatSession,
	listChatHistory,
	updateChatSession,
} from "@/lib/api";
import { getChatPrefetchLimit } from "@/lib/app-settings";
import { createPiSessionId, normalizeWorkspacePath } from "@/lib/session-utils";
import { getWsManager } from "@/lib/ws-manager";
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
import { useLocale } from "./ui-context";

export interface ChatContextValue {
	/** Chat sessions from disk (no running opencode needed) */
	chatHistory: ChatSession[];
	/** Error message when chat history service is unavailable */
	chatHistoryError: string | null;
	selectedChatSessionId: string | null;
	setSelectedChatSessionId: (id: string | null) => void;
	/** Get the selected chat from history. */
	selectedChatFromHistory: ChatSession | undefined;
	/** Set of chat session IDs that are currently busy (agent working) */
	busySessions: Set<string>;
	/** Mark a session as busy or idle */
	setSessionBusy: (sessionId: string, busy: boolean) => void;
	refreshChatHistory: () => Promise<void>;
	/** Create a placeholder chat session for instant UI feedback. */
	createOptimisticChatSession: (
		sessionId: string,
		workspacePath?: string,
	) => string;
	/** Remove a placeholder chat session. */
	clearOptimisticChatSession: (sessionId: string) => void;
	/** Replace a placeholder chat session with the real session id. */
	replaceOptimisticChatSession: (
		optimisticId: string,
		sessionId: string,
	) => void;
	/** Update a chat session title locally without triggering backend rename. */
	updateChatSessionTitleLocal: (
		sessionId: string,
		title: string,
		readableId?: string | null,
	) => void;
	createNewChat: (
		workspacePath?: string,
		options?: { optimisticId?: string },
	) => Promise<string | null>;
	deleteChatSession: (sessionId: string) => Promise<boolean>;
	renameChatSession: (sessionId: string, title: string) => Promise<boolean>;
}

const noop = () => {};
const asyncNoop = async () => null;
const asyncNoopVoid = async () => {};
const asyncNoopBool = async () => false;

const CHAT_HISTORY_CACHE_KEY = "octo:chatHistoryCache:v2";
const CHAT_HISTORY_CACHE_MAX_CHARS = 2_000_000;
const CHAT_HISTORY_PREFETCH_DEBOUNCE_MS = 2000;

function readCachedChatHistory(): ChatSession[] {
	if (typeof window === "undefined") return [];
	try {
		const raw = localStorage.getItem(CHAT_HISTORY_CACHE_KEY);
		if (!raw) return [];
		if (raw.length > CHAT_HISTORY_CACHE_MAX_CHARS) {
			localStorage.removeItem(CHAT_HISTORY_CACHE_KEY);
			return [];
		}
		const parsed = JSON.parse(raw) as ChatSession[];
		if (!Array.isArray(parsed)) return [];
		return parsed.map((session) => ({
			...session,
			workspace_path: normalizeWorkspacePath(session.workspace_path),
		}));
	} catch {
		return [];
	}
}

function writeCachedChatHistory(history: ChatSession[]) {
	if (typeof window === "undefined") return;
	try {
		const encoded = JSON.stringify(history);
		if (encoded.length > CHAT_HISTORY_CACHE_MAX_CHARS) {
			localStorage.removeItem(CHAT_HISTORY_CACHE_KEY);
			return;
		}
		localStorage.setItem(CHAT_HISTORY_CACHE_KEY, encoded);
	} catch {
		// ignore storage failures
	}
}

const defaultChatContext: ChatContextValue = {
	chatHistory: [],
	chatHistoryError: null,
	selectedChatSessionId: null,
	setSelectedChatSessionId: noop,
	selectedChatFromHistory: undefined,
	busySessions: new Set(),
	setSessionBusy: noop,
	refreshChatHistory: asyncNoopVoid,
	createOptimisticChatSession: (_sessionId?: string) => "",
	clearOptimisticChatSession: noop,
	replaceOptimisticChatSession: noop,
	updateChatSessionTitleLocal: noop,
	createNewChat: asyncNoop,
	deleteChatSession: asyncNoopBool,
	renameChatSession: asyncNoopBool,
};

const ChatContext = createContext<ChatContextValue>(defaultChatContext);

export function ChatProvider({ children }: { children: ReactNode }) {
	const { locale } = useLocale();

	const [chatHistory, setChatHistory] = useState<ChatSession[]>(() =>
		readCachedChatHistory(),
	);
	const [chatHistoryError, setChatHistoryError] = useState<string | null>(null);
	const chatHistoryErrorRef = useRef<string | null>(null);
	chatHistoryErrorRef.current = chatHistoryError;
	const chatHistoryRef = useRef<ChatSession[]>([]);
	const optimisticChatSessionsRef = useRef<Map<string, ChatSession>>(new Map());
	const optimisticSelectionRef = useRef<Map<string, string | null>>(new Map());
	const lastPrefetchRef = useRef(0);
	const prefetchInFlightRef = useRef(false);
	chatHistoryRef.current = chatHistory;

	const [selectedChatSessionId, setSelectedChatSessionIdRaw] = useState<
		string | null
	>(() => {
		if (typeof window !== "undefined") {
			try {
				return localStorage.getItem("octo:lastChatSessionId") || null;
			} catch {
				return null;
			}
		}
		return null;
	});

	const setSelectedChatSessionId = useCallback(
		(value: string | null | ((prev: string | null) => string | null)) => {
			setSelectedChatSessionIdRaw((prev) => {
				const newId = typeof value === "function" ? value(prev) : value;
				if (typeof window !== "undefined") {
					try {
						if (newId?.trim()) {
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

	const selectedChatFromHistory = useMemo(() => {
		return chatHistory.find((s) => s.id === selectedChatSessionId);
	}, [chatHistory, selectedChatSessionId]);

	// Auto-select the most recently active session from hstry when
	// nothing valid is selected (first load or stale localStorage value).
	const autoSelectedRef = useRef(false);
	useEffect(() => {
		if (autoSelectedRef.current) return;
		if (chatHistory.length === 0) return;
		if (
			selectedChatSessionId &&
			chatHistory.some((s) => s.id === selectedChatSessionId)
		)
			return;
		// Pick the session with the highest updated_at
		let best = chatHistory[0];
		for (let i = 1; i < chatHistory.length; i++) {
			if (chatHistory[i].updated_at > best.updated_at) best = chatHistory[i];
		}
		autoSelectedRef.current = true;
		setSelectedChatSessionId(best.id);
	}, [chatHistory, selectedChatSessionId, setSelectedChatSessionId]);

	const mergeOptimisticSessions = useCallback((history: ChatSession[]) => {
		if (optimisticChatSessionsRef.current.size === 0) return history;
		const optimistic = Array.from(optimisticChatSessionsRef.current.values());
		const byId = new Map(history.map((s) => [s.id, s]));
		for (const session of optimistic) {
			if (!byId.has(session.id)) {
				byId.set(session.id, session);
			}
		}
		return Array.from(byId.values());
	}, []);

	const normalizeHistory = useCallback((history: ChatSession[]) => {
		return history.map((session) => {
			const normalized = normalizeWorkspacePath(session.workspace_path);
			return {
				...session,
				workspace_path: normalized ?? null,
			};
		});
	}, []);

	const refreshChatHistory = useCallback(async () => {
		const prefetchLimit = getChatPrefetchLimit();
		const now = Date.now();
		if (prefetchInFlightRef.current) return;
		// Bypass debounce when there's an active error (user clicking Retry)
		const hasError = chatHistoryErrorRef.current !== null;
		if (
			!hasError &&
			now - lastPrefetchRef.current < CHAT_HISTORY_PREFETCH_DEBOUNCE_MS
		) {
			return;
		}
		prefetchInFlightRef.current = true;
		lastPrefetchRef.current = now;
		try {
			const history = await listChatHistory(prefetchLimit);
			const normalized = normalizeHistory(history);
			const merged = mergeOptimisticSessions(normalized);
			startTransition(() => {
				setChatHistory(merged);
				setChatHistoryError(null);
			});
			writeCachedChatHistory(merged);
		} catch (err) {
			const msg =
				err instanceof Error ? err.message : "Failed to load chat history";
			console.error("[chat-context] refreshChatHistory failed:", msg);
			startTransition(() => {
				setChatHistoryError(msg);
			});
		} finally {
			prefetchInFlightRef.current = false;
		}
	}, [mergeOptimisticSessions, normalizeHistory]);

	useEffect(() => {
		refreshChatHistory();
	}, [refreshChatHistory]);

	const createOptimisticChatSession = useCallback(
		(sessionId: string, workspacePath?: string) => {
			const optimisticId = sessionId;
			if (optimisticChatSessionsRef.current.has(optimisticId)) {
				return optimisticId;
			}
			const resolvedPath = normalizeWorkspacePath(workspacePath);
			// Derive a client-side project name from the workspace path
			// (last path component), matching the backend's logic in
			// project_name_from_path(). Without this, optimistic sessions
			// have project_name=null and the sidebar falls back to
			// "Workspace" as the group label.
			const derivedProjectName = resolvedPath
				? (resolvedPath.replace(/\\/g, "/").split("/").filter(Boolean).pop() ??
					null)
				: null;
			const session: ChatSession = {
				id: optimisticId,
				readable_id: null,
				title: locale === "de" ? "Neue Sitzung" : "New Session",
				parent_id: null,
				workspace_path: resolvedPath ?? null,
				project_name: derivedProjectName,
				created_at: Date.now(),
				updated_at: Date.now(),
				version: 1,
				is_child: false,
				source_path: null,
			};
			optimisticChatSessionsRef.current.set(optimisticId, session);
			optimisticSelectionRef.current.set(optimisticId, selectedChatSessionId);
			startTransition(() => {
				setChatHistory((prev) => [session, ...prev]);
			});
			return optimisticId;
		},
		[locale, selectedChatSessionId],
	);

	const clearOptimisticChatSession = useCallback((sessionId: string) => {
		optimisticChatSessionsRef.current.delete(sessionId);
		optimisticSelectionRef.current.delete(sessionId);
		setChatHistory((prev) => prev.filter((s) => s.id !== sessionId));
	}, []);

	const replaceOptimisticChatSession = useCallback(
		(optimisticId: string, sessionId: string) => {
			const optimistic = optimisticChatSessionsRef.current.get(optimisticId);
			optimisticChatSessionsRef.current.delete(optimisticId);
			optimisticSelectionRef.current.delete(optimisticId);
			if (!optimistic) return;
			const next: ChatSession = { ...optimistic, id: sessionId };
			setChatHistory((prev) =>
				prev.map((s) => (s.id === optimisticId ? next : s)),
			);
			if (selectedChatSessionId === optimisticId) {
				setSelectedChatSessionId(sessionId);
			}
		},
		[selectedChatSessionId, setSelectedChatSessionId],
	);

	const updateChatSessionTitleLocal = useCallback(
		(sessionId: string, title: string, readableId?: string | null) => {
			if (!title.trim()) return;
			setChatHistory((prev) =>
				prev.map((s) =>
					s.id === sessionId
						? {
								...s,
								title,
								...(readableId != null ? { readable_id: readableId } : {}),
							}
						: s,
				),
			);
		},
		[],
	);

	const createNewChat = useCallback(
		async (workspacePath?: string) => {
			const resolvedPath = normalizeWorkspacePath(workspacePath) ?? null;
			const sessionId = createPiSessionId();
			createOptimisticChatSession(sessionId, resolvedPath);
			setSelectedChatSessionId(sessionId);
			void refreshChatHistory();
			return sessionId;
		},
		[createOptimisticChatSession, refreshChatHistory, setSelectedChatSessionId],
	);

	const deleteChatSession = useCallback(
		async (sessionId: string) => {
			try {
				// Close the agent session via WS (if active)
				try {
					getWsManager().agentCloseSession(sessionId);
				} catch {
					// Session may not be active, that's fine
				}
				setChatHistory((prev) => prev.filter((s) => s.id !== sessionId));
				if (selectedChatSessionId === sessionId) {
					setSelectedChatSessionId(null);
				}
				return true;
			} catch {
				return false;
			}
		},
		[selectedChatSessionId, setSelectedChatSessionId],
	);

	const renameChatSession = useCallback(
		async (sessionId: string, title: string): Promise<boolean> => {
			try {
				const updated = await updateChatSession(sessionId, { title });
				setChatHistory((prev) =>
					prev.map((s) =>
						s.id === sessionId ? { ...s, title: updated.title } : s,
					),
				);
				return true;
			} catch {
				return false;
			}
		},
		[],
	);

	const value = useMemo<ChatContextValue>(
		() => ({
			chatHistory,
			chatHistoryError,
			selectedChatSessionId,
			setSelectedChatSessionId,
			selectedChatFromHistory,
			busySessions,
			setSessionBusy,
			refreshChatHistory,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			replaceOptimisticChatSession,
			updateChatSessionTitleLocal,
			createNewChat,
			deleteChatSession,
			renameChatSession,
		}),
		[
			chatHistory,
			chatHistoryError,
			selectedChatSessionId,
			setSelectedChatSessionId,
			selectedChatFromHistory,
			busySessions,
			setSessionBusy,
			refreshChatHistory,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			replaceOptimisticChatSession,
			updateChatSessionTitleLocal,
			createNewChat,
			deleteChatSession,
			renameChatSession,
		],
	);

	return <ChatContext.Provider value={value}>{children}</ChatContext.Provider>;
}

export function useChatContext() {
	return useContext(ChatContext);
}

export function useChatHistory() {
	return useChatContext().chatHistory;
}

export function useSelectedChat() {
	const { selectedChatFromHistory, selectedChatSessionId } = useChatContext();
	return { selectedChatFromHistory, selectedChatSessionId };
}

export function useBusySessions() {
	const { busySessions, setSessionBusy } = useChatContext();
	return { busySessions, setSessionBusy };
}
