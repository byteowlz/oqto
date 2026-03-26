"use client";

import {
	type ChatSession,
	deleteChatSessionApi,
	listChatHistory,
	updateChatSession,
} from "@/lib/api";
import {
	createPiSessionId,
	isGenericSessionTitle,
	normalizeWorkspacePath,
	preferStableSessionTitle,
} from "@/lib/session-utils";
import { getWsManager } from "@/lib/ws-manager";
import type { WsMuxConnectionState } from "@/lib/ws-mux-types";

/**
 * Module-level map of sessionId -> sharedWorkspaceId.
 * Populated by createOptimisticChatSession when a shared workspace session is clicked.
 * Consumed by useChat's fetchHistoryMessages to route REST calls to the correct runner.
 */
const SHARED_SESSION_MAP_STORAGE_KEY = "oqto:shared-session-map:v1";

export const sharedWorkspaceSessionMap = new Map<string, string>();
const runnerHistoryAliasMap = new Map<string, string>();

export function getRunnerHistoryAlias(sessionId: string): string | undefined {
	return runnerHistoryAliasMap.get(sessionId);
}

function persistSharedWorkspaceSessionMap() {
	if (typeof window === "undefined") return;
	try {
		localStorage.setItem(
			SHARED_SESSION_MAP_STORAGE_KEY,
			JSON.stringify(Object.fromEntries(sharedWorkspaceSessionMap.entries())),
		);
	} catch {
		// ignore storage failures
	}
}

function hydrateSharedWorkspaceSessionMap() {
	if (typeof window === "undefined") return;
	if (sharedWorkspaceSessionMap.size > 0) return;
	try {
		const raw = localStorage.getItem(SHARED_SESSION_MAP_STORAGE_KEY);
		if (!raw) return;
		const parsed = JSON.parse(raw) as Record<string, string>;
		for (const [sessionId, workspaceId] of Object.entries(parsed)) {
			if (sessionId && workspaceId) {
				sharedWorkspaceSessionMap.set(sessionId, workspaceId);
			}
		}
	} catch {
		// ignore parse/storage failures
	}
}

export function setSharedWorkspaceSessionId(
	sessionId: string,
	sharedWorkspaceId: string,
) {
	sharedWorkspaceSessionMap.set(sessionId, sharedWorkspaceId);
	persistSharedWorkspaceSessionMap();
}

export function clearSharedWorkspaceSessionId(sessionId: string) {
	sharedWorkspaceSessionMap.delete(sessionId);
	persistSharedWorkspaceSessionMap();
}

if (typeof window !== "undefined") {
	hydrateSharedWorkspaceSessionMap();
}

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
import { useTranslation } from "react-i18next";

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
		shared_workspace_id?: string;
		hstry_id?: string;
	}>;
	/** Count of active Pi sessions on the runner */
	runnerSessionCount: number;
	refreshChatHistory: () => Promise<void>;
	/** Create a placeholder chat session for instant UI feedback. */
	createOptimisticChatSession: (
		sessionId: string,
		workspacePath?: string,
		sharedWorkspaceId?: string,
		existingSession?: ChatSession,
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
	createNewChat: (workspacePath?: string) => Promise<string | null>;
	deleteChatSession: (sessionId: string) => Promise<boolean>;
	renameChatSession: (sessionId: string, title: string) => Promise<boolean>;
	getSessionWorkspacePath: (sessionId: string | null) => string | null;
}

const noop = () => {};
const asyncNoop = async () => null;
const asyncNoopVoid = async () => {};
const asyncNoopBool = async () => false;

const CHAT_HISTORY_CACHE_KEY = "oqto:chatHistoryCache:v2";
const CHAT_HISTORY_CACHE_MAX_CHARS = 2_000_000;
const CHAT_HISTORY_PREFETCH_DEBOUNCE_MS = 2000;
const RUNNER_SESSIONS_POLL_MS = 5000;
const RUNNER_SESSIONS_POLL_HIDDEN_MS = 20000;

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
	createOptimisticChatSession: (
		_sessionId?: string,
		_workspacePath?: string,
		_sharedWorkspaceId?: string,
		_existingSession?: ChatSession,
	) => "",
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
	const { t } = useTranslation();

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

	// Track auto-generated titles from session.title_changed events.
	// These are kept until hstry confirms the title (returns the same
	// non-generic title), preventing race conditions where a refresh
	// from hstry overwrites a title that hasn't been persisted yet.
	const autoTitlesRef = useRef<Map<string, string>>(new Map());

	const [selectedChatSessionId, setSelectedChatSessionIdRaw] = useState<
		string | null
	>(() => {
		if (typeof window === "undefined") return null;
		// Restore the last session the user was viewing. The auto-select
		// effect will only override this if the saved ID doesn't exist in
		// the session list (e.g. deleted session).
		try {
			return localStorage.getItem("oqto:lastChatSessionId") || null;
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
							localStorage.setItem("oqto:lastChatSessionId", newId);
						} else {
							localStorage.removeItem("oqto:lastChatSessionId");
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
	// Track deleted session IDs to prevent resurrection from hstry/runner polls.
	const deletedSessionsRef = useRef<Set<string>>(new Set());
	const [runnerSessions, setRunnerSessions] = useState<
		Array<{
			session_id: string;
			state: string;
			cwd: string;
			provider?: string;
			model?: string;
			last_activity: number;
			subscriber_count: number;
			shared_workspace_id?: string;
			hstry_id?: string;
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
	// Populate sharedWorkspaceSessionMap from runner sessions so that
	// shared workspace sessions are recognized even after page reload.
	useEffect(() => {
		for (const rs of runnerSessions) {
			if (rs.shared_workspace_id) {
				setSharedWorkspaceSessionId(rs.session_id, rs.shared_workspace_id);
			}
		}
	}, [runnerSessions]);
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
			const optimistic = Array.from(optimisticChatSessionsRef.current.values());
			const byId = new Map(history.map((s) => [s.id, s]));
			const byReadable = new Map(
				history
					.filter(
						(s): s is ChatSession & { readable_id: string } =>
							!!s.readable_id?.trim(),
					)
					.map((s) => [s.readable_id.trim(), s]),
			);

			for (const session of optimistic) {
				// hstry now returns the Oqto session ID (platform_id) as the
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
				// Skip sessions that belong to shared workspaces -- they render
				// under the shared workspace section, not personal sessions.
				if ((session as Record<string, unknown>).shared_workspace_id) continue;
				// Skip sessions that were explicitly deleted
				if (deletedSessionsRef.current.has(session.session_id)) continue;

				const historyAliasId = session.hstry_id?.trim();
				if (historyAliasId && historyAliasId !== session.session_id) {
					runnerHistoryAliasMap.set(session.session_id, historyAliasId);
					const aliasEntry = byId.get(historyAliasId);
					if (aliasEntry && !byId.has(session.session_id)) {
						byId.delete(historyAliasId);
						byId.set(session.session_id, {
							...aliasEntry,
							id: session.session_id,
						});
					}
				}
				// hstry now returns platform_id (Oqto ID) as the session id,
				// but some legacy rows still resolve via hstry_id aliases.
				if (byId.has(session.session_id)) continue;

				const resolvedPath = normalizeWorkspacePath(session.cwd);
				const derivedProjectName = resolvedPath
					? (resolvedPath
							.replace(/\\/g, "/")
							.split("/")
							.filter(Boolean)
							.pop() ?? null)
					: null;
				const timestamp = session.last_activity || Date.now();
				const existing = chatHistoryRef.current.find(
					(s) =>
						s.id === session.session_id ||
						(historyAliasId ? s.id === historyAliasId : false),
				);
				const title =
					preferStableSessionTitle(
						existing?.title,
						t("sessions.activeSession"),
						t("sessions.newSession"),
						t("sessions.activeSession"),
					) ?? t("sessions.activeSession");

				byId.set(session.session_id, {
					id: session.session_id,
					readable_id: existing?.readable_id ?? null,
					title,
					parent_id: existing?.parent_id ?? null,
					workspace_path: resolvedPath ?? existing?.workspace_path ?? null,
					project_name: derivedProjectName ?? existing?.project_name ?? null,
					created_at: existing?.created_at ?? timestamp,
					updated_at: timestamp,
					version: existing?.version ?? null,
					is_child: existing?.is_child ?? false,
					source_path: existing?.source_path ?? null,
					model: session.model ?? existing?.model ?? null,
					provider: session.provider ?? existing?.provider ?? null,
				});
			}

			return Array.from(byId.values());
		},
		[t, runnerSessions],
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
				if (
					activeIds.has(session.id) &&
					!byId.has(session.id) &&
					!deletedSessionsRef.current.has(session.id)
				) {
					byId.set(session.id, session);
				}
			}

			return Array.from(byId.values());
		},
		[selectedChatSessionId],
	);

	const refreshChatHistory = useCallback(
		async (opts?: { force?: boolean }) => {
			const now = Date.now();
			if (prefetchInFlightRef.current) {
				if (isPiDebugEnabled())
					console.debug("[chat-context] refreshChatHistory skipped: in-flight");
				return;
			}
			// Bypass debounce when there's an active error (user clicking Retry)
			const hasError = chatHistoryErrorRef.current !== null;
			if (
				!opts?.force &&
				!hasError &&
				now - lastPrefetchRef.current < CHAT_HISTORY_PREFETCH_DEBOUNCE_MS
			) {
				if (isPiDebugEnabled())
					console.debug("[chat-context] refreshChatHistory skipped: debounce");
				return;
			}
			prefetchInFlightRef.current = true;
			lastPrefetchRef.current = now;
			const t0 = performance.now();
			try {
				const rawHistory = await listChatHistory();
				// Filter out sessions that were explicitly deleted in this page session
				const history =
					deletedSessionsRef.current.size > 0
						? rawHistory.filter((s) => !deletedSessionsRef.current.has(s.id))
						: rawHistory;
				const t1 = performance.now();
				const normalized = normalizeHistory(history);
				const merged = mergeRunnerSessions(mergeOptimisticSessions(normalized));
				const withActive = mergeActiveSessions(merged);
				if (isPiDebugEnabled()) {
					console.debug(
						"[chat-context] refreshChatHistory: fetched",
						history.length,
						"sessions in",
						`${Math.round(t1 - t0)}ms, total`,
						`${Math.round(performance.now() - t0)}ms`,
					);
				}
				// Preserve titles for sessions that were manually renamed by the user
				// or auto-generated by the extension but not yet confirmed by hstry.
				const manualTitles = manuallyRenamedRef.current;
				const autoTitles = autoTitlesRef.current;
				const final_ = withActive.map((s) => {
					const manualTitle = manualTitles.get(s.id);
					if (manualTitle) {
						return { ...s, title: manualTitle };
					}
					const autoTitle = autoTitles.get(s.id);
					if (autoTitle) {
						// hstry has caught up — clear the override
						if (s.title === autoTitle) {
							autoTitles.delete(s.id);
							return s;
						}
						// hstry hasn't caught up yet — keep the auto title
						return { ...s, title: autoTitle };
					}
					const previous = chatHistoryRef.current.find(
						(prev) => prev.id === s.id,
					);
					if (!previous) return s;
					return {
						...s,
						title: preferStableSessionTitle(
							previous.title,
							s.title,
							t("sessions.newSession"),
							t("sessions.activeSession"),
						),
					};
				});
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
		},
		[
			mergeActiveSessions,
			mergeOptimisticSessions,
			mergeRunnerSessions,
			normalizeHistory,
			t,
		],
	);

	useEffect(() => {
		refreshChatHistory();
	}, [refreshChatHistory]);

	useEffect(() => {
		if (runnerSessions.length === 0) return;
		const current = chatHistoryRef.current;
		let hasMissing = false;
		for (const session of runnerSessions) {
			// Shared workspace sessions are rendered in a separate sidebar section
			// and intentionally skipped by mergeRunnerSessions(). They must not
			// trigger the personal hstry refresh loop here.
			if (session.shared_workspace_id) continue;
			if (!current.some((item) => item.id === session.session_id)) {
				hasMissing = true;
				break;
			}
		}
		// Also check if any runner session still has a generic title in the
		// chat history -- this means hstry hasn't been queried since the
		// runner first reported the session.
		let hasGenericTitle = false;
		if (!hasMissing) {
			for (const session of runnerSessions) {
				if (session.shared_workspace_id) continue;
				const entry = current.find((item) => item.id === session.session_id);
				if (
					entry &&
					isGenericSessionTitle(
						entry.title,
						t("sessions.newSession"),
						t("sessions.activeSession"),
					)
				) {
					hasGenericTitle = true;
					break;
				}
			}
		}
		if (hasMissing) {
			const merged = mergeRunnerSessions(current);
			setChatHistory(merged);
			writeCachedChatHistory(merged);
		}
		// Fetch real titles from hstry for new or generically-titled sessions
		if (hasMissing || hasGenericTitle) {
			void refreshChatHistory({ force: true });
		}
	}, [mergeRunnerSessions, refreshChatHistory, runnerSessions, t]);

	// Poll runner for active Pi sessions via the mux WebSocket.
	// Keeps busy indicators accurate across reloads and backend restarts.
	useEffect(() => {
		const manager = getWsManager();
		let pollTimer: ReturnType<typeof setTimeout> | null = null;
		let cancelled = false;
		let pollInFlight = false;

		const busyStates = new Set([
			"streaming",
			"compacting",
			"starting",
			"aborting",
		]);

		const scheduleNextPoll = () => {
			if (cancelled) return;
			if (pollTimer) {
				clearTimeout(pollTimer);
			}
			const hidden = typeof document !== "undefined" && document.hidden;
			const delay = hidden
				? RUNNER_SESSIONS_POLL_HIDDEN_MS
				: RUNNER_SESSIONS_POLL_MS;
			pollTimer = setTimeout(() => {
				void pollSessions();
			}, delay);
		};

		const pollSessions = async () => {
			if (pollInFlight) return;
			pollInFlight = true;
			try {
				const sessions = await manager.agentListSessions();
				if (cancelled) return;
				// Filter out sessions that were explicitly deleted in this page session
				const filtered =
					deletedSessionsRef.current.size > 0
						? sessions.filter(
								(s) => !deletedSessionsRef.current.has(s.session_id),
							)
						: sessions;
				setRunnerSessions(filtered);
				for (const s of filtered) {
					const alias = s.hstry_id?.trim();
					if (alias && alias !== s.session_id) {
						runnerHistoryAliasMap.set(s.session_id, alias);
					}
				}
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
					console.debug("[chat-context] Could not list active sessions:", err);
				}
			} finally {
				pollInFlight = false;
				scheduleNextPoll();
			}
		};

		const onVisibilityChange = () => {
			scheduleNextPoll();
		};
		if (typeof document !== "undefined") {
			document.addEventListener("visibilitychange", onVisibilityChange);
		}

		const unsubscribe = manager.onConnectionState(
			(state: WsMuxConnectionState) => {
				if (state === "connected") {
					void pollSessions();
				} else if (pollTimer) {
					clearTimeout(pollTimer);
					pollTimer = null;
				}
			},
		);

		return () => {
			cancelled = true;
			unsubscribe();
			if (pollTimer) {
				clearTimeout(pollTimer);
			}
			if (typeof document !== "undefined") {
				document.removeEventListener("visibilitychange", onVisibilityChange);
			}
		};
	}, []);

	const createOptimisticChatSession = useCallback(
		(
			sessionId: string,
			workspacePath?: string,
			sharedWorkspaceId?: string,
			existingSession?: ChatSession,
		) => {
			const optimisticId = sessionId;
			if (optimisticChatSessionsRef.current.has(optimisticId)) {
				if (existingSession) {
					const mergedExisting: ChatSession = {
						...existingSession,
						shared_workspace_id:
							sharedWorkspaceId ?? existingSession.shared_workspace_id ?? null,
					};
					optimisticChatSessionsRef.current.set(optimisticId, mergedExisting);
					startTransition(() => {
						setChatHistory((prev) =>
							prev.map((s) =>
								s.id === optimisticId ? { ...s, ...mergedExisting } : s,
							),
						);
					});
				}
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
			// Track shared workspace association for REST API routing
			if (sharedWorkspaceId) {
				setSharedWorkspaceSessionId(optimisticId, sharedWorkspaceId);
			}

			// If an existing session was provided (e.g. clicking an existing
			// shared workspace session), carry over its full metadata so
			// the chat header shows the correct title, readable_id, etc.
			const session: ChatSession = existingSession
				? {
						...existingSession,
						shared_workspace_id:
							sharedWorkspaceId ?? existingSession.shared_workspace_id ?? null,
					}
				: {
						id: optimisticId,
						readable_id: null,
						title: t("sessions.newSession"),
						parent_id: null,
						workspace_path: resolvedPath ?? null,
						project_name: derivedProjectName,
						created_at: Date.now(),
						updated_at: Date.now(),
						version: null,
						is_child: false,
						source_path: null,
						shared_workspace_id: sharedWorkspaceId ?? null,
					};
			optimisticChatSessionsRef.current.set(optimisticId, session);
			optimisticSelectionRef.current.set(optimisticId, selectedChatSessionId);
			startTransition(() => {
				setChatHistory((prev) => {
					// If the session already exists, update it in-place without
					// clobbering existing title/readable_id metadata.
					const idx = prev.findIndex((s) => s.id === optimisticId);
					if (idx >= 0) {
						const current = prev[idx];
						const safeSession = existingSession
							? session
							: {
									...session,
									title: current.title ?? session.title,
									readable_id: current.readable_id ?? session.readable_id,
								};
						const updated = [...prev];
						updated[idx] = { ...current, ...safeSession };
						return updated;
					}
					return [session, ...prev];
				});
			});
			return optimisticId;
		},
		[t, selectedChatSessionId],
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
						"auto:",
						title,
						"manual:",
						manuallyRenamedRef.current.get(sessionId),
					);
				}
				return;
			}
			// Store in autoTitlesRef so refreshChatHistory preserves
			// this title until hstry confirms it.
			autoTitlesRef.current.set(sessionId, title);
			setChatHistory((prev) =>
				prev.map((s) => {
					if (s.id !== sessionId) return s;
					return {
						...s,
						title,
						...(readableId != null ? { readable_id: readableId } : {}),
					};
				}),
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
		async (workspacePath?: string, sharedWorkspaceId?: string) => {
			let resolvedPath = normalizeWorkspacePath(workspacePath) ?? null;
			if (!resolvedPath && selectedChatSessionId) {
				resolvedPath = getSessionWorkspacePath(selectedChatSessionId);
			}
			const sessionId = createPiSessionId();
			createOptimisticChatSession(
				sessionId,
				resolvedPath ?? undefined,
				sharedWorkspaceId,
			);
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
			const sessionMeta = chatHistory.find((s) => s.id === sessionId);
			const sharedWorkspaceId =
				sharedWorkspaceSessionMap.get(sessionId) ??
				sessionMeta?.shared_workspace_id ??
				undefined;

			// Mark deleted before any async work so runner/history refreshes cannot
			// resurrect the session while the delete request is in flight.
			deletedSessionsRef.current.add(sessionId);
			optimisticChatSessionsRef.current.delete(sessionId);
			optimisticSelectionRef.current.delete(sessionId);
			clearSharedWorkspaceSessionId(sessionId);
			setRunnerSessions((prev) =>
				prev.filter((session) => session.session_id !== sessionId),
			);
			setBusySessions((prev) => {
				if (!prev.has(sessionId)) return prev;
				const next = new Set(prev);
				next.delete(sessionId);
				return next;
			});
			// Optimistically remove from UI immediately
			setChatHistory((prev) => prev.filter((s) => s.id !== sessionId));
			if (selectedChatSessionId === sessionId) {
				setSelectedChatSessionId(null);
			}

			try {
				if (sharedWorkspaceId) {
					setSharedWorkspaceSessionId(sessionId, sharedWorkspaceId);
				}

				// Primary path: REST delete with shared workspace routing support.
				await deleteChatSessionApi(sessionId, sharedWorkspaceId);

				// Best-effort: also notify active WS session state machine.
				try {
					const manager = getWsManager();
					manager.send({
						channel: "agent",
						session_id: sessionId,
						cmd: "session.delete",
					});
				} catch {
					// ignore (session may not be connected)
				}
				clearSharedWorkspaceSessionId(sessionId);

				return true;
			} catch {
				// Delete failed: allow session to reappear from source of truth.
				deletedSessionsRef.current.delete(sessionId);
				if (sharedWorkspaceId) {
					setSharedWorkspaceSessionId(sessionId, sharedWorkspaceId);
				}
				void refreshChatHistory();
				return false;
			}
		},
		[
			chatHistory,
			refreshChatHistory,
			selectedChatSessionId,
			setSelectedChatSessionId,
		],
	);

	const renameChatSession = useCallback(
		async (sessionId: string, title: string): Promise<boolean> => {
			try {
				const sessionMeta = chatHistoryRef.current.find(
					(s) => s.id === sessionId,
				);
				const sharedWorkspaceId =
					sharedWorkspaceSessionMap.get(sessionId) ??
					sessionMeta?.shared_workspace_id ??
					undefined;
				if (sharedWorkspaceId) {
					setSharedWorkspaceSessionId(sessionId, sharedWorkspaceId);
				}

				const updated = await updateChatSession(
					sessionId,
					{ title },
					sharedWorkspaceId,
				);

				const canonicalSessionId = updated.id || sessionId;
				const canonicalTitle = (updated.title ?? title).trim();
				const runnerAlias = runnerSessionsRef.current.find(
					(s) =>
						s.session_id === canonicalSessionId ||
						s.hstry_id === canonicalSessionId ||
						s.session_id === sessionId ||
						s.hstry_id === sessionId,
				);
				const runnerSessionId = runnerAlias?.session_id;

				// If backend canonicalized an optimistic ID, update local mappings.
				if (canonicalSessionId !== sessionId) {
					const existingManualTitle = manuallyRenamedRef.current.get(sessionId);
					if (
						existingManualTitle &&
						!manuallyRenamedRef.current.has(canonicalSessionId)
					) {
						manuallyRenamedRef.current.set(
							canonicalSessionId,
							existingManualTitle,
						);
					}
					manuallyRenamedRef.current.delete(sessionId);
					autoTitlesRef.current.delete(sessionId);
					const override = sessionWorkspaceOverridesRef.current.get(sessionId);
					if (override !== undefined) {
						sessionWorkspaceOverridesRef.current.set(
							canonicalSessionId,
							override,
						);
						sessionWorkspaceOverridesRef.current.delete(sessionId);
					}
					if (sharedWorkspaceId) {
						setSharedWorkspaceSessionId(canonicalSessionId, sharedWorkspaceId);
						clearSharedWorkspaceSessionId(sessionId);
					}
					replaceOptimisticChatSession(sessionId, canonicalSessionId);
				}

				// Mark this session as manually renamed so auto-generated
				// title events from Pi don't overwrite the user's choice.
				if (canonicalTitle) {
					manuallyRenamedRef.current.set(canonicalSessionId, canonicalTitle);
					manuallyRenamedRef.current.set(sessionId, canonicalTitle);
					if (runnerSessionId) {
						manuallyRenamedRef.current.set(runnerSessionId, canonicalTitle);
					}
				}
				setChatHistory((prev) =>
					prev.map((s) =>
						s.id === canonicalSessionId ||
						s.id === sessionId ||
						(runnerSessionId != null && s.id === runnerSessionId)
							? { ...s, id: canonicalSessionId, title: canonicalTitle }
							: s,
					),
				);

				// Also tell the runner/Pi to update its internal session name.
				// This prevents the runner from overwriting hstry with Pi's
				// auto-generated title on the next state event.
				const manager = getWsManager();
				const runnerTargetId = runnerSessionId ?? canonicalSessionId;
				if (manager.isSessionReady(runnerTargetId)) {
					void manager
						.agentSetSessionName(runnerTargetId, canonicalTitle)
						.catch(() => {
							// Best-effort -- runner notification is not critical.
						});
				}

				return true;
			} catch {
				return false;
			}
		},
		[replaceOptimisticChatSession],
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
