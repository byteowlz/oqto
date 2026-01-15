"use client";

import {
	type ChatSession,
	type ProjectEntry,
	type WorkspaceSession,
	createWorkspaceSession,
	deleteWorkspaceSession,
	getMainChatAssistant,
	getOrCreateSessionForWorkspace,
	getOrCreateWorkspaceSession,
	listChatHistory,
	listProjects,
	listWorkspaceSessions,
	opencodeProxyBaseUrl,
	stopWorkspaceSession,
	touchSessionActivity,
	updateChatSession,
	upgradeWorkspaceSession,
} from "@/lib/control-plane-client";
import {
	type OpenCodeSession,
	type Persona,
	createSession,
	deleteSession,
	fetchSessions,
	updateSession,
} from "@/lib/opencode-client";
import { generateReadableId } from "@/lib/session-utils";
import { type WsEvent, getWsClient } from "@/lib/ws-client";
import {
	type Dispatch,
	type ReactNode,
	type SetStateAction,
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

interface SessionContextValue {
	workspaceSessions: WorkspaceSession[];
	selectedWorkspaceSessionId: string;
	setSelectedWorkspaceSessionId: (id: string) => void;
	selectedWorkspaceSession: WorkspaceSession | undefined;
	opencodeBaseUrl: string;
	opencodeDirectory?: string;
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
	refreshWorkspaceSessions: () => Promise<void>;
	refreshChatHistory: () => Promise<void>;
	refreshOpencodeSessions: () => Promise<void>;
	/** Create a placeholder chat session for instant UI feedback. */
	createOptimisticChatSession: (workspacePath?: string) => string;
	/** Remove a placeholder chat session. */
	clearOptimisticChatSession: (sessionId: string) => void;
	/** Ensure opencode is running and return the base URL. Starts if needed. */
	ensureOpencodeRunning: (workspacePath?: string) => Promise<string | null>;
	createNewChat: (
		baseUrlOverride?: string,
		directoryOverride?: string,
		options?: { optimisticId?: string },
	) => Promise<OpenCodeSession | null>;
	createNewChatWithPersona: (
		persona: Persona,
		workspacePath?: string,
	) => Promise<OpenCodeSession | null>;
	deleteChatSession: (
		sessionId: string,
		baseUrlOverride?: string,
	) => Promise<boolean>;
	renameChatSession: (sessionId: string, title: string) => Promise<boolean>;
	stopWorkspaceSession: (sessionId: string) => Promise<boolean>;
	deleteWorkspaceSession: (sessionId: string) => Promise<boolean>;
	upgradeWorkspaceSession: (sessionId: string) => Promise<boolean>;
	/** Available projects (directories in workspace_dir) */
	projects: ProjectEntry[];
	/** Start a new session for a project */
	startProjectSession: (
		projectPath: string,
	) => Promise<WorkspaceSession | null>;
	projectDefaultAgents: Record<string, string>;
	setProjectDefaultAgents: Dispatch<SetStateAction<Record<string, string>>>;
	/** Main Chat state - when active, shows threaded view of all Main Chat sessions */
	mainChatActive: boolean;
	setMainChatActive: (active: boolean) => void;
	mainChatAssistantName: string | null;
	setMainChatAssistantName: (name: string | null) => void;
	/** The current Main Chat session ID to send messages to */
	mainChatCurrentSessionId: string | null;
	setMainChatCurrentSessionId: (id: string | null) => void;
	/** Workspace path for the Main Chat assistant */
	mainChatWorkspacePath: string | null;
	setMainChatWorkspacePath: (path: string | null) => void;
	/** Target message ID to scroll to after navigation (from search) */
	scrollToMessageId: string | null;
	setScrollToMessageId: (id: string | null) => void;
}

const SessionContext = createContext<SessionContextValue | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
	const { locale } = useLocale();

	const [workspaceSessions, setWorkspaceSessions] = useState<
		WorkspaceSession[]
	>([]);
	const workspaceSessionsRef = useRef<WorkspaceSession[]>([]);
	workspaceSessionsRef.current = workspaceSessions;
	const [selectedWorkspaceSessionId, setSelectedWorkspaceSessionId] =
		useState<string>("");
	// Chat history from disk (no running opencode needed)
	const [chatHistory, setChatHistory] = useState<ChatSession[]>([]);
	const chatHistoryRef = useRef<ChatSession[]>([]);
	const optimisticChatSessionsRef = useRef<Map<string, ChatSession>>(
		new Map(),
	);
	const optimisticSelectionRef = useRef<Map<string, string>>(new Map());
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
	// Available projects
	const [projects, setProjects] = useState<ProjectEntry[]>([]);
	const [projectDefaultAgents, setProjectDefaultAgents] = useState<
		Record<string, string>
	>(() => {
		if (typeof window === "undefined") return {};
		try {
			const stored = localStorage.getItem("octo:projectDefaultAgents");
			return stored ? JSON.parse(stored) : {};
		} catch {
			return {};
		}
	});
	// Track which chat sessions are currently busy (agent working)
	const [busySessions, setBusySessions] = useState<Set<string>>(new Set());
	// Main Chat state - restore from localStorage, default to main chat if no last session
	const [mainChatActive, setMainChatActiveRaw] = useState(() => {
		if (typeof window !== "undefined") {
			try {
				const lastSessionId = localStorage.getItem("octo:lastChatSessionId");
				const lastMainChatActive = localStorage.getItem("octo:lastMainChatActive");
				// If we have a stored preference, use it
				if (lastMainChatActive !== null) {
					return lastMainChatActive === "true";
				}
				// If we have a last session ID, default to opencode mode
				if (lastSessionId) {
					return false;
				}
			} catch {
				// Ignore localStorage errors
			}
		}
		// Default to main chat if nothing stored
		return true;
	});
	const setMainChatActive = useCallback(
		(value: boolean | ((prev: boolean) => boolean)) => {
			startTransition(() => {
				setMainChatActiveRaw((prev) => {
					const newValue = typeof value === "function" ? value(prev) : value;
					// Persist to localStorage
					if (typeof window !== "undefined") {
						try {
							localStorage.setItem("octo:lastMainChatActive", String(newValue));
						} catch {
							// Ignore localStorage errors
						}
					}
					return newValue;
				});
			});
		},
		[],
	);
	const [mainChatAssistantName, setMainChatAssistantName] = useState<
		string | null
	>(null);
	const [mainChatCurrentSessionId, setMainChatCurrentSessionId] = useState<
		string | null
	>(null);
	// Main chat workspace path - cached to localStorage for instant load
	const [mainChatWorkspacePath, setMainChatWorkspacePathRaw] = useState<
		string | null
	>(null);
	// Wrap setter to also cache to localStorage
	const setMainChatWorkspacePath = useCallback((path: string | null) => {
		setMainChatWorkspacePathRaw(path);
		if (typeof window !== "undefined") {
			try {
				if (path) {
					localStorage.setItem("mainChatWorkspacePath", path);
				} else {
					localStorage.removeItem("mainChatWorkspacePath");
				}
			} catch {
				// Ignore localStorage errors
			}
		}
	}, []);

	const [scrollToMessageId, setScrollToMessageId] = useState<string | null>(
		null,
	);

	// Initialize main chat workspace path when mainChatActive becomes true
	// Use localStorage cache for instant load, then refresh from API
	useEffect(() => {
		if (!mainChatActive) {
			// Clear workspace path when leaving main chat mode
			setMainChatWorkspacePathRaw(null);
			return;
		}
		
		// Try to load from cache first for instant display
		if (typeof window !== "undefined") {
			try {
				const cached = localStorage.getItem("mainChatWorkspacePath");
				if (cached) {
					setMainChatWorkspacePathRaw(cached);
				}
			} catch {
				// Ignore localStorage errors
			}
		}
		
		// Then fetch fresh data from API
		let cancelled = false;
		getMainChatAssistant("default")
			.then((info) => {
				if (!cancelled) {
					setMainChatWorkspacePath(info.path);
					if (!mainChatAssistantName) {
						setMainChatAssistantName(info.name);
					}
				}
			})
			.catch((err) => {
				console.debug("[MainChat] No main chat configured:", err.message);
				// Clear cached path if main chat is not configured
				setMainChatWorkspacePath(null);
			});
		return () => {
			cancelled = true;
		};
	}, [mainChatActive, mainChatAssistantName, setMainChatWorkspacePath]);

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

	const selectedWorkspaceSession = useMemo(() => {
		if (!selectedWorkspaceSessionId) return undefined;
		return workspaceSessions.find(
			(session) => session.id === selectedWorkspaceSessionId,
		);
	}, [selectedWorkspaceSessionId, workspaceSessions]);

	const opencodeBaseUrl = useMemo(() => {
		if (typeof window !== "undefined") {
			const params = new URLSearchParams(window.location.search);
			const mockUrl = params.get("mockOpencode");
			if (mockUrl) {
				console.log("[Dev] Using mock OpenCode server:", mockUrl);
				return mockUrl;
			}
		}
		if (!selectedWorkspaceSession) return "";
		if (selectedWorkspaceSession.status !== "running") return "";
		return opencodeProxyBaseUrl(selectedWorkspaceSession.id);
	}, [selectedWorkspaceSession]);

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
				(entry) =>
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
				readable_id: generateReadableId(optimisticId),
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
		[locale, opencodeDirectory, resolveProjectName, selectedChatSessionId, setSelectedChatSessionId],
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

	const sessionEventSubscriptions = useRef(new Map<string, () => void>());

	useEffect(() => {
		try {
			const storedWorkspaceSessionId =
				window.localStorage.getItem("workspaceSessionId") ?? "";
			if (storedWorkspaceSessionId) {
				setSelectedWorkspaceSessionId(storedWorkspaceSessionId);
			}
		} catch {
			// Ignore storage failures.
		}
	}, []);

	// Refresh chat history from disk
	const refreshChatHistory = useCallback(async () => {
		try {
			const history = await listChatHistory({ include_children: true });
			startTransition(() => {
				const optimisticSessions = Array.from(
					optimisticChatSessionsRef.current.values(),
				).filter(
					(session) => !history.some((item) => item.id === session.id),
				);
				setChatHistory(
					optimisticSessions.length > 0
						? [...optimisticSessions, ...history]
						: history,
				);

				if (history.length > 0 && !mainChatActive) {
					setSelectedChatSessionId((current) => {
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
	}, [mainChatActive, setSelectedChatSessionId]);

	const refreshWorkspaceSessions = useCallback(async () => {
		try {
			const [sessionsData, projectsData] = await Promise.all([
				listWorkspaceSessions().catch(() => [] as WorkspaceSession[]),
				listProjects().catch(() => [] as ProjectEntry[]),
			]);

			startTransition(() => {
				setWorkspaceSessions(sessionsData);
				setProjects(projectsData);

				if (sessionsData.length > 0) {
					setSelectedWorkspaceSessionId((current) => {
						if (!current) {
							const running = sessionsData.find(
								(s) => s.status === "running",
							);
							return running?.id || sessionsData[0].id;
						}
						const currentSession = sessionsData.find(
							(s) => s.id === current,
						);
						if (!currentSession) {
							return sessionsData[0].id;
						}
						return current;
					});
				}
			});
		} catch (err) {
			console.error("Failed to load sessions:", err);
		}
	}, []);

	// Debounced refresh
	const refreshTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const debouncedRefreshWorkspaceSessions = useCallback(() => {
		if (refreshTimeoutRef.current) {
			clearTimeout(refreshTimeoutRef.current);
		}
		refreshTimeoutRef.current = setTimeout(() => {
			refreshTimeoutRef.current = null;
			refreshWorkspaceSessions();
		}, 500);
	}, [refreshWorkspaceSessions]);

	// Handle WebSocket events
	const handleWsEvent = useCallback(
		(event: WsEvent) => {
			const sessionId =
				"session_id" in event ? (event.session_id as string) : undefined;

			if (event.type === "session_busy" && sessionId) {
				setSessionBusy(sessionId, true);
			} else if (event.type === "session_idle" && sessionId) {
				setSessionBusy(sessionId, false);
			}

			if (event.type === "session_updated" && sessionId) {
				const knownSession = workspaceSessionsRef.current.some(
					(s) => s.id === sessionId,
				);
				if (!knownSession) {
					debouncedRefreshWorkspaceSessions();
					return;
				}

				startTransition(() => {
					setWorkspaceSessions((current) => {
						const idx = current.findIndex((s) => s.id === sessionId);
						if (idx === -1) return current;

						const existing = current[idx];
						if (
							existing.status === event.status &&
							existing.workspace_path === event.workspace_path
						) {
							return current;
						}

						const next = [...current];
						next[idx] = {
							...existing,
							status: event.status as WorkspaceSession["status"],
							workspace_path: event.workspace_path,
						};
						return next;
					});
				});
				return;
			}

			if (
				event.type === "session_deleted" ||
				event.type === "agent_connected" ||
				event.type === "agent_disconnected"
			) {
				debouncedRefreshWorkspaceSessions();
			}
		},
		[debouncedRefreshWorkspaceSessions, setSessionBusy],
	);

	const startProjectSession = useCallback(
		async (projectPath: string): Promise<WorkspaceSession | null> => {
			try {
				const session = await getOrCreateSessionForWorkspace(projectPath);
				await refreshWorkspaceSessions();
				setSelectedWorkspaceSessionId(session.id);
				return session;
			} catch (err) {
				console.error("Failed to start project session:", err);
				return null;
			}
		},
		[refreshWorkspaceSessions],
	);

	const ensureOpencodeRunning = useCallback(
		async (workspacePath?: string): Promise<string | null> => {
			try {
				let session: WorkspaceSession;

				if (workspacePath) {
					session = await getOrCreateSessionForWorkspace(workspacePath);
				} else {
					if (selectedWorkspaceSession?.status === "running") {
						touchSessionActivity(selectedWorkspaceSession.id).catch(() => {});
						return opencodeProxyBaseUrl(selectedWorkspaceSession.id);
					}
					session = await getOrCreateWorkspaceSession();
				}

				await refreshWorkspaceSessions();
				setSelectedWorkspaceSessionId(session.id);

				if (session.status === "running") {
					return opencodeProxyBaseUrl(session.id);
				}

				let attempts = 0;
				const maxAttempts = 30;
				while (attempts < maxAttempts) {
					const sessions = await listWorkspaceSessions();
					const current = sessions.find((s) => s.id === session.id);
					if (current?.status === "running") {
						return opencodeProxyBaseUrl(current.id);
					}
					if (current?.status === "failed") {
						console.error(
							"Workspace session failed to start:",
							current.error_message,
						);
						return null;
					}
					attempts++;
					await new Promise((resolve) => setTimeout(resolve, 1000));
				}

				console.error("Timeout waiting for workspace session to start");
				return null;
			} catch (err) {
				console.error("Failed to ensure opencode is running:", err);
				return null;
			}
		},
		[selectedWorkspaceSession, refreshWorkspaceSessions],
	);

	useEffect(() => {
		refreshWorkspaceSessions();
		refreshChatHistory();
	}, [refreshWorkspaceSessions, refreshChatHistory]);

	// Subscribe to WebSocket events for all running sessions
	useEffect(() => {
		const wsClient = getWsClient();
		const runningSessions = workspaceSessions.filter(
			(session) => session.status === "running",
		);
		const activeIds = new Set(runningSessions.map((session) => session.id));

		for (const session of runningSessions) {
			if (sessionEventSubscriptions.current.has(session.id)) {
				continue;
			}
			wsClient.subscribeSession(session.id);
			const unsubscribe = wsClient.onSessionEvent(session.id, handleWsEvent);
			sessionEventSubscriptions.current.set(session.id, unsubscribe);
		}

		for (const [sessionId, unsubscribe] of sessionEventSubscriptions.current) {
			if (!activeIds.has(sessionId)) {
				unsubscribe();
				wsClient.unsubscribeSession(sessionId);
				sessionEventSubscriptions.current.delete(sessionId);
			}
		}
	}, [workspaceSessions, handleWsEvent]);

	// Cleanup on unmount
	useEffect(() => {
		return () => {
			const wsClient = getWsClient();
			for (const [
				sessionId,
				unsubscribe,
			] of sessionEventSubscriptions.current) {
				unsubscribe();
				wsClient.unsubscribeSession(sessionId);
			}
			sessionEventSubscriptions.current.clear();
		};
	}, []);

	useEffect(() => {
		if (!selectedWorkspaceSession) return;
		if (
			selectedWorkspaceSession.status === "starting" ||
			selectedWorkspaceSession.status === "pending"
		) {
			const timeout = setTimeout(() => {
				void refreshWorkspaceSessions();
			}, 1000);
			return () => clearTimeout(timeout);
		}
	}, [selectedWorkspaceSession, refreshWorkspaceSessions]);

	useEffect(() => {
		if (!selectedWorkspaceSessionId) return;
		try {
			window.localStorage.setItem(
				"workspaceSessionId",
				selectedWorkspaceSessionId,
			);
		} catch {
			// Ignore storage failures.
		}
	}, [selectedWorkspaceSessionId]);

	useEffect(() => {
		if (typeof window === "undefined") return;
		try {
			localStorage.setItem(
				"octo:projectDefaultAgents",
				JSON.stringify(projectDefaultAgents),
			);
		} catch {
			// Ignore storage failures.
		}
	}, [projectDefaultAgents]);

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
	}, [mainChatActive, opencodeBaseUrl, opencodeDirectory, setSelectedChatSessionId]);

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
				setSelectedChatSessionId(created.id);
				setTimeout(() => {
					refreshChatHistory();
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

	const createNewChatWithPersona = useCallback(
		async (
			persona: Persona,
			workspacePath?: string,
		): Promise<OpenCodeSession | null> => {
			try {
				const basePath = selectedWorkspaceSession?.workspace_path;
				const resolvePath = (path?: string) => {
					if (!path) return undefined;
					if (path.startsWith("/")) return path;
					if (!basePath) return path;
					if (path === "." || path.trim() === "") return basePath;
					const joined = `${basePath}/${path}`;
					const normalized = joined.split("/").filter(Boolean).join("/");
					return basePath.startsWith("/") ? `/${normalized}` : normalized;
				};
				const resolvedPath =
					workspacePath ?? resolvePath(persona.default_workdir || undefined);

				const workspaceSession = await createWorkspaceSession({
					persona_id: persona.id,
					workspace_path: resolvedPath,
				});

				await refreshWorkspaceSessions();
				setSelectedWorkspaceSessionId(workspaceSession.id);

				const baseUrl = opencodeProxyBaseUrl(workspaceSession.id);

				let attempts = 0;
				const maxAttempts = 30;
				while (attempts < maxAttempts) {
					try {
						const created = await createSession(baseUrl, undefined, undefined, {
							directory: resolvedPath,
						});
						setOpencodeSessions((prev) => [created, ...prev]);
						setSelectedChatSessionId(created.id);
						setTimeout(() => {
							refreshChatHistory();
						}, 500);
						return created;
					} catch {
						attempts++;
						await new Promise((resolve) => setTimeout(resolve, 1000));
					}
				}

				console.error("Timeout waiting for workspace session to be ready");
				return null;
			} catch (err) {
				console.error("Failed to create new chat with persona:", err);
				return null;
			}
		},
		[
			refreshWorkspaceSessions,
			refreshChatHistory,
			selectedWorkspaceSession?.workspace_path,
			setSelectedChatSessionId,
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
					const remaining = opencodeSessions.filter(
						(s) => s.id !== sessionId,
					);
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

	const handleStopWorkspaceSession = useCallback(
		async (sessionId: string): Promise<boolean> => {
			try {
				await stopWorkspaceSession(sessionId);
				await refreshWorkspaceSessions();
				return true;
			} catch (err) {
				console.error("Failed to stop workspace session:", err);
				return false;
			}
		},
		[refreshWorkspaceSessions],
	);

	const handleDeleteWorkspaceSession = useCallback(
		async (sessionId: string): Promise<boolean> => {
			try {
				await deleteWorkspaceSession(sessionId);
				await refreshWorkspaceSessions();
				return true;
			} catch (err) {
				console.error("Failed to delete workspace session:", err);
				return false;
			}
		},
		[refreshWorkspaceSessions],
	);

	const handleUpgradeWorkspaceSession = useCallback(
		async (sessionId: string): Promise<boolean> => {
			try {
				await upgradeWorkspaceSession(sessionId);
				await refreshWorkspaceSessions();
				return true;
			} catch (err) {
				console.error("Failed to upgrade workspace session:", err);
				return false;
			}
		},
		[refreshWorkspaceSessions],
	);

	useEffect(() => {
		refreshOpencodeSessions();
	}, [refreshOpencodeSessions]);

	const value = useMemo(
		() => ({
			workspaceSessions,
			selectedWorkspaceSessionId,
			setSelectedWorkspaceSessionId,
			selectedWorkspaceSession,
			opencodeBaseUrl,
			opencodeDirectory,
			chatHistory,
			opencodeSessions,
			selectedChatSessionId,
			setSelectedChatSessionId,
			selectedChatSession,
			selectedChatFromHistory,
			busySessions,
			setSessionBusy,
			refreshWorkspaceSessions,
			refreshChatHistory,
			refreshOpencodeSessions,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			ensureOpencodeRunning,
			createNewChat,
			createNewChatWithPersona,
			deleteChatSession,
			renameChatSession,
			stopWorkspaceSession: handleStopWorkspaceSession,
			deleteWorkspaceSession: handleDeleteWorkspaceSession,
			upgradeWorkspaceSession: handleUpgradeWorkspaceSession,
			projects,
			startProjectSession,
			projectDefaultAgents,
			setProjectDefaultAgents,
			mainChatActive,
			setMainChatActive,
			mainChatAssistantName,
			setMainChatAssistantName,
			mainChatCurrentSessionId,
			setMainChatCurrentSessionId,
			mainChatWorkspacePath,
			setMainChatWorkspacePath,
			scrollToMessageId,
			setScrollToMessageId,
		}),
		[
			workspaceSessions,
			selectedWorkspaceSessionId,
			selectedWorkspaceSession,
			opencodeBaseUrl,
			opencodeDirectory,
			chatHistory,
			opencodeSessions,
			selectedChatSessionId,
			setSelectedChatSessionId,
			selectedChatSession,
			selectedChatFromHistory,
			busySessions,
			setSessionBusy,
			refreshWorkspaceSessions,
			refreshChatHistory,
			refreshOpencodeSessions,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			ensureOpencodeRunning,
			createNewChat,
			createNewChatWithPersona,
			deleteChatSession,
			renameChatSession,
			handleStopWorkspaceSession,
			handleDeleteWorkspaceSession,
			handleUpgradeWorkspaceSession,
			projects,
			startProjectSession,
			projectDefaultAgents,
			mainChatActive,
			setMainChatActive,
			mainChatAssistantName,
			mainChatCurrentSessionId,
			mainChatWorkspacePath,
			setMainChatWorkspacePath,
			scrollToMessageId,
		],
	);

	return (
		<SessionContext.Provider value={value}>
			{children}
		</SessionContext.Provider>
	);
}

export function useSessionContext() {
	const context = useContext(SessionContext);
	if (!context) {
		throw new Error("useSessionContext must be used within a SessionProvider");
	}
	return context;
}

// Selective hooks for performance - only subscribe to what you need

export function useBusySessions() {
	const { busySessions, setSessionBusy } = useSessionContext();
	return { busySessions, setSessionBusy };
}

export function useChatHistory() {
	const { chatHistory, refreshChatHistory } = useSessionContext();
	return { chatHistory, refreshChatHistory };
}

export function useSelectedChat() {
	const {
		selectedChatSessionId,
		setSelectedChatSessionId,
		selectedChatSession,
		selectedChatFromHistory,
	} = useSessionContext();
	return {
		selectedChatSessionId,
		setSelectedChatSessionId,
		selectedChatSession,
		selectedChatFromHistory,
	};
}

export function useWorkspaceSessions() {
	const {
		workspaceSessions,
		selectedWorkspaceSessionId,
		setSelectedWorkspaceSessionId,
		selectedWorkspaceSession,
		refreshWorkspaceSessions,
	} = useSessionContext();
	return {
		workspaceSessions,
		selectedWorkspaceSessionId,
		setSelectedWorkspaceSessionId,
		selectedWorkspaceSession,
		refreshWorkspaceSessions,
	};
}

export function useMainChat() {
	const {
		mainChatActive,
		setMainChatActive,
		mainChatAssistantName,
		setMainChatAssistantName,
		mainChatCurrentSessionId,
		setMainChatCurrentSessionId,
		mainChatWorkspacePath,
		setMainChatWorkspacePath,
	} = useSessionContext();
	return {
		mainChatActive,
		setMainChatActive,
		mainChatAssistantName,
		setMainChatAssistantName,
		mainChatCurrentSessionId,
		setMainChatCurrentSessionId,
		mainChatWorkspacePath,
		setMainChatWorkspacePath,
	};
}
