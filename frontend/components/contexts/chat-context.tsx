"use client";

import {
	type ChatSession,
	listChatHistory,
	updateChatSession,
} from "@/lib/api";
import {
	createPiSessionId,
	normalizeWorkspacePath,
} from "@/lib/session-utils";
import type { WsMuxConnectionState } from "@/lib/ws-mux-types";
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

function isPiDebugEnabled(): boolean {
	if (!import.meta.env.DEV) return false;
	try {
		if (typeof localStorage !== "undefined") {
			return localStorage.getItem("debug:pi-v2") === "1";
		}
	} catch {
		// ignore
	}
	return import.meta.env.VITE_DEBUG_PI_V2 === "1";
}

export interface ChatContextValue {
	/** Chat sessions from disk (read from hstry) */
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
	/** Currently active Pi sessions reported by the runner */
	runnerSessions: Array<{
		session_id: string;
		state: string;
		cwd: string;
		provider?: string;
		model?: string;
		last_activity: number;
		subscriber_count: number;
	}>;
	/** Count of active Pi sessions on the runner */
	runnerSessionCount: number;
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
	getSessionWorkspacePath: (sessionId: string | null) => string | null;
}

const noop = () => {};
const asyncNoop = async () => null;
const asyncNoopVoid = async () => {};
const asyncNoopBool = async () => false;

const CHAT_HISTORY_CACHE_KEY = "octo:chatHistoryCache:v2";
const CHAT_HISTORY_CACHE_MAX_CHARS = 2_000_000;
const CHAT_HISTORY_PREFETCH_DEBOUNCE_MS = 2000;
const RUNNER_SESSIONS_POLL_MS = 5000;

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
	runnerSessions: [],
	runnerSessionCount: 0,
	refreshChatHistory: asyncNoopVoid,
	createOptimisticChatSession: (_sessionId?: string) => "",
	clearOptimisticChatSession: noop,
	replaceOptimisticChatSession: noop,
	updateChatSessionTitleLocal: noop,
	createNewChat: asyncNoop,
	deleteChatSession: asyncNoopBool,
	renameChatSession: asyncNoopBool,
	getSessionWorkspacePath: () => null,
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
	const sessionWorkspaceOverridesRef = useRef<Map<string, string | null>>(
		new Map(),
	);

	const lastPrefetchRef = useRef(0);
	const prefetchInFlightRef = useRef(false);
	chatHistoryRef.current = chatHistory;

	// Track sessions that have been manually renamed by the user.
	// Auto-generated title events from Pi are ignored for these sessions.
	const manuallyRenamedRef = useRef<Map<string, string>>(new Map());

	const [selectedChatSessionId, setSelectedChatSessionIdRaw] = useState<
		string | null
	>(() => {
		if (typeof window === "undefined") return null;
		// Restore the last session the user was viewing. The auto-select
		// effect will only override this if the saved ID doesn't exist in
		// the session list (e.g. deleted session).
		try {
			return localStorage.getItem("octo:lastChatSessionId") || null;
		} catch {
			return null;
		}
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
	const [runnerSessions, setRunnerSessions] = useState<
		Array<{
			session_id: string;
			state: string;
			cwd: string;
			provider?: string;
			model?: string;
			last_activity: number;
			subscriber_count: number;
		}>
	>([]);
	const runnerSessionsRef = useRef(runnerSessions);
	runnerSessionsRef.current = runnerSessions;

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

	// Auto-select a session only when the current selection is invalid
	// (null or not found in chatHistory). This preserves the user's last
	// viewed session across reloads. When we do need to pick, prefer an
	// active runner session, then fall back to the most recently updated.
	const autoSelectedRef = useRef(false);
	const runnerSessionsById = useMemo(
		() => new Map(runnerSessions.map((s) => [s.session_id, s])),
		[runnerSessions],
	);
	useEffect(() => {
		if (autoSelectedRef.current) return;
		if (chatHistory.length === 0) return;

		// If the saved session exists in the list, keep it
		if (
			selectedChatSessionId &&
			chatHistory.some((s) => s.id === selectedChatSessionId)
		) {
			autoSelectedRef.current = true;
			return;
		}

		// Saved session is missing/invalid -- pick a new one.
		// Prefer an active runner session if available
		const activeCandidates = chatHistory.filter((s) =>
			runnerSessionsById.has(s.id),
		);
		if (activeCandidates.length > 0) {
			let best = activeCandidates[0];
			let bestActivity = runnerSessionsById.get(best.id)?.last_activity ?? 0;
			for (let i = 1; i < activeCandidates.length; i++) {
				const current = activeCandidates[i];
				const activity = runnerSessionsById.get(current.id)?.last_activity ?? 0;
				if (activity > bestActivity) {
					best = current;
					bestActivity = activity;
				}
			}
			autoSelectedRef.current = true;
			setSelectedChatSessionId(best.id);
			return;
		}

		// Fallback: pick the session with the highest updated_at
		let best = chatHistory[0];
		for (let i = 1; i < chatHistory.length; i++) {
			if (chatHistory[i].updated_at > best.updated_at) best = chatHistory[i];
		}
		autoSelectedRef.current = true;
		setSelectedChatSessionId(best.id);
	}, [
		chatHistory,
		selectedChatSessionId,
		runnerSessionsById,
		setSelectedChatSessionId,
	]);

	const mergeOptimisticSessions = useCallback(
		(history: ChatSession[]) => {
			if (optimisticChatSessionsRef.current.size === 0) return history;
			const optimistic = Array.from(
				optimisticChatSessionsRef.current.values(),
			);
			const byId = new Map(history.map((s) => [s.id, s]));
			const byReadable = new Map(
				history
					.filter((s) => s.readable_id?.trim())
					.map((s) => [s.readable_id.trim(), s]),
			);

			for (const session of optimistic) {
				// hstry now returns the Octo session ID (platform_id) as the
				// session id, so byId.has() matches directly -- no cross-ID
				// dedup needed.
				if (byId.has(session.id)) {
					// hstry has the real entry; retire the optimistic placeholder
					optimisticChatSessionsRef.current.delete(session.id);
					optimisticSelectionRef.current.delete(session.id);
					continue;
				}
				const replacement = byReadable.get(session.id);
				if (replacement) {
					optimisticChatSessionsRef.current.delete(session.id);
					optimisticSelectionRef.current.delete(session.id);
					if (selectedChatSessionId === session.id) {
						setSelectedChatSessionId(replacement.id);
					}
					continue;
				}
				byId.set(session.id, session);
			}
			return Array.from(byId.values());
		},
		[selectedChatSessionId, setSelectedChatSessionId],
	);

	const mergeRunnerSessions = useCallback(
		(history: ChatSession[]) => {
			if (runnerSessions.length === 0) return history;
			const byId = new Map(history.map((s) => [s.id, s]));

			for (const session of runnerSessions) {
				// hstry now returns platform_id (Octo ID) as the session id,
				// so a direct match is sufficient.
				if (byId.has(session.session_id)) continue;

				const resolvedPath = normalizeWorkspacePath(session.cwd);
				const derivedProjectName = resolvedPath
					? (resolvedPath.replace(/\\/g, "/").split("/").filter(Boolean).pop() ??
							null)
					: null;
				const timestamp = session.last_activity || Date.now();

				byId.set(session.session_id, {
					id: session.session_id,
					readable_id: null,
					title: locale === "de" ? "Aktive Sitzung" : "Active Session",
					parent_id: null,
					workspace_path: resolvedPath ?? null,
					project_name: derivedProjectName,
					created_at: timestamp,
					updated_at: timestamp,
					version: null,
					is_child: false,
					source_path: null,
					model: session.model ?? null,
					provider: session.provider ?? null,
				});
			}

			return Array.from(byId.values());
		},
		[locale, runnerSessions],
	);

	const normalizeHistory = useCallback((history: ChatSession[]) => {
		return history.map((session) => {
			const normalized = normalizeWorkspacePath(session.workspace_path);
			return {
				...session,
				workspace_path: normalized ?? null,
			};
		});
	}, []);

	const mergeActiveSessions = useCallback(
		(history: ChatSession[]) => {
			if (!selectedChatSessionId && runnerSessionsRef.current.length === 0) {
				return history;
			}

			const byId = new Map(history.map((session) => [session.id, session]));
			const activeIds = new Set<string>();
			if (selectedChatSessionId) {
				activeIds.add(selectedChatSessionId);
			}
			for (const session of runnerSessionsRef.current) {
				activeIds.add(session.session_id);
			}

			if (activeIds.size === 0) return history;

			for (const session of chatHistoryRef.current) {
				if (activeIds.has(session.id) && !byId.has(session.id)) {
					byId.set(session.id, session);
				}
			}

			return Array.from(byId.values());
		},
		[selectedChatSessionId],
	);

	const refreshChatHistory = useCallback(async () => {
		const now = Date.now();
		if (prefetchInFlightRef.current) {
			if (isPiDebugEnabled()) console.debug("[chat-context] refreshChatHistory skipped: in-flight");
			return;
		}
		// Bypass debounce when there's an active error (user clicking Retry)
		const hasError = chatHistoryErrorRef.current !== null;
		if (
			!hasError &&
			now - lastPrefetchRef.current < CHAT_HISTORY_PREFETCH_DEBOUNCE_MS
		) {
			if (isPiDebugEnabled()) console.debug("[chat-context] refreshChatHistory skipped: debounce");
			return;
		}
		prefetchInFlightRef.current = true;
		lastPrefetchRef.current = now;
		const t0 = performance.now();
		try {
			const history = await listChatHistory();
			const t1 = performance.now();
			const normalized = normalizeHistory(history);
			const merged = mergeRunnerSessions(mergeOptimisticSessions(normalized));
			const withActive = mergeActiveSessions(merged);
			if (isPiDebugEnabled()) {
				console.debug("[chat-context] refreshChatHistory: fetched", history.length,
					"sessions in", Math.round(t1 - t0) + "ms, total",
					Math.round(performance.now() - t0) + "ms");
			}
			// Preserve titles for sessions that were manually renamed by the user.
			// The runner may have overwritten hstry with an auto-generated title
			// between the user's rename and this refresh.
			const manualTitles = manuallyRenamedRef.current;
			const final_ = manualTitles.size > 0
				? withActive.map((s) => {
						const manualTitle = manualTitles.get(s.id);
						return manualTitle ? { ...s, title: manualTitle } : s;
					})
				: withActive;
			setChatHistory(final_);
			setChatHistoryError(null);
			writeCachedChatHistory(final_);
		} catch (err) {
			const msg =
				err instanceof Error ? err.message : "Failed to load chat history";
			console.error("[chat-context] refreshChatHistory failed:", msg);
			setChatHistoryError(msg);
		} finally {
			prefetchInFlightRef.current = false;
		}
	}, [mergeActiveSessions, mergeOptimisticSessions, mergeRunnerSessions, normalizeHistory]);

	useEffect(() => {
		refreshChatHistory();
	}, [refreshChatHistory]);

	useEffect(() => {
		if (runnerSessions.length === 0) return;
		const current = chatHistoryRef.current;
		let hasMissing = false;
		for (const session of runnerSessions) {
			if (!current.some((item) => item.id === session.session_id)) {
				hasMissing = true;
				break;
			}
		}
		if (!hasMissing) return;
		const merged = mergeRunnerSessions(current);
		setChatHistory(merged);
		writeCachedChatHistory(merged);
	}, [mergeRunnerSessions, runnerSessions]);

	// Poll runner for active Pi sessions via the mux WebSocket.
	// Keeps busy indicators accurate across reloads and backend restarts.
	useEffect(() => {
		const manager = getWsManager();
		let pollTimer: ReturnType<typeof setInterval> | null = null;
		let cancelled = false;

		const busyStates = new Set([
			"streaming",
			"compacting",
			"starting",
			"aborting",
		]);

		const pollSessions = async () => {
			try {
				const sessions = await manager.agentListSessions();
				if (cancelled) return;
				setRunnerSessions(sessions);
				const nextBusy = new Set<string>();
				for (const s of sessions) {
					if (busyStates.has(s.state)) {
						nextBusy.add(s.session_id);
					}
				}
				setBusySessions(nextBusy);
				if (isPiDebugEnabled()) {
					console.debug(
						"[chat-context] Runner sessions:",
						sessions.length,
						"busy:",
						nextBusy.size,
					);
				}
			} catch (err) {
				if (isPiDebugEnabled()) {
					console.debug(
						"[chat-context] Could not list active sessions:",
						err,
					);
				}
			}
		};

		const unsubscribe = manager.onConnectionState(
			(state: WsMuxConnectionState) => {
				if (state === "connected") {
					pollSessions();
					if (!pollTimer) {
						pollTimer = setInterval(pollSessions, RUNNER_SESSIONS_POLL_MS);
					}
				} else if (pollTimer) {
					clearInterval(pollTimer);
					pollTimer = null;
				}
			},
		);

		return () => {
			cancelled = true;
			unsubscribe();
			if (pollTimer) {
				clearInterval(pollTimer);
			}
		};
	}, []);

	const createOptimisticChatSession = useCallback(
		(sessionId: string, workspacePath?: string) => {
			const optimisticId = sessionId;
			if (optimisticChatSessionsRef.current.has(optimisticId)) {
				return optimisticId;
			}
			const resolvedPath = normalizeWorkspacePath(workspacePath);
			sessionWorkspaceOverridesRef.current.set(
				optimisticId,
				resolvedPath ?? null,
			);
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
			// If this session was manually renamed by the user, ignore
			// auto-generated title events from Pi to preserve the user's choice.
			if (manuallyRenamedRef.current.has(sessionId)) {
				if (isPiDebugEnabled()) {
					console.debug(
						"[chat-context] Ignoring auto-title for manually renamed session:",
						sessionId,
						"auto:", title,
						"manual:", manuallyRenamedRef.current.get(sessionId),
					);
				}
				return;
			}
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

	const getSessionWorkspacePath = useCallback((sessionId: string | null) => {
		if (!sessionId) return null;
		const override = sessionWorkspaceOverridesRef.current.get(sessionId);
		if (override !== undefined) return override;
		const historyEntry = chatHistoryRef.current.find(
			(session) => session.id === sessionId,
		);
		if (historyEntry?.workspace_path) return historyEntry.workspace_path;
		const runnerEntry = runnerSessionsRef.current.find(
			(session) => session.session_id === sessionId,
		);
		return normalizeWorkspacePath(runnerEntry?.cwd) ?? null;
	}, []);

	const createNewChat = useCallback(
		async (workspacePath?: string) => {
			let resolvedPath = normalizeWorkspacePath(workspacePath) ?? null;
			if (!resolvedPath && selectedChatSessionId) {
				resolvedPath = getSessionWorkspacePath(selectedChatSessionId);
			}
			const sessionId = createPiSessionId();
			createOptimisticChatSession(sessionId, resolvedPath);
			setSelectedChatSessionId(sessionId);
			void refreshChatHistory();
			return sessionId;
		},
		[
			createOptimisticChatSession,
			getSessionWorkspacePath,
			refreshChatHistory,
			selectedChatSessionId,
			setSelectedChatSessionId,
		],
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
				// Mark this session as manually renamed so auto-generated
				// title events from Pi don't overwrite the user's choice.
				manuallyRenamedRef.current.set(sessionId, updated.title);
				setChatHistory((prev) =>
					prev.map((s) =>
						s.id === sessionId ? { ...s, title: updated.title } : s,
					),
				);

				// Also tell the runner/Pi to update its internal session name.
				// This prevents the runner from overwriting hstry with Pi's
				// auto-generated title on the next state event.
				try {
					const manager = getWsManager();
					if (manager.isConnected) {
						void manager.agentSetSessionName(sessionId, updated.title);
					}
				} catch {
					// Best-effort -- runner notification is not critical
				}

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
			runnerSessions,
			runnerSessionCount: runnerSessions.length,
			refreshChatHistory,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			replaceOptimisticChatSession,
			updateChatSessionTitleLocal,
			getSessionWorkspacePath,
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
			runnerSessions,
			refreshChatHistory,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			replaceOptimisticChatSession,
			updateChatSessionTitleLocal,
			getSessionWorkspacePath,
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
