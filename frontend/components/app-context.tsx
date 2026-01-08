"use client";

import {
	type AppDefinition,
	type Locale,
	type LocalizedText,
	appRegistry,
} from "@/lib/app-registry";
import {
	type ChatSession,
	type Persona,
	type ProjectEntry,
	type WorkspaceSession,
	controlPlaneDirectBaseUrl,
	createWorkspaceSession,
	deleteWorkspaceSession,
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
import { i18n, resolveStoredLocale } from "@/lib/i18n";
import {
	type OpenCodeSession,
	createSession,
	deleteSession,
	fetchSessions,
	updateSession,
} from "@/lib/opencode-client";
import { type WsEvent, getWsClient } from "@/lib/ws-client";
import {
	type Dispatch,
	type ReactNode,
	type SetStateAction,
	createContext,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";

interface AppContextValue {
	apps: AppDefinition[];
	activeAppId: string;
	setActiveAppId: (id: string) => void;
	activeApp?: AppDefinition;
	locale: Locale;
	setLocale: (locale: Locale) => void;
	resolveText: (value: LocalizedText) => string;
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
	/** Ensure opencode is running and return the base URL. Starts if needed.
	 * If workspacePath is provided, ensures a session for that specific workspace.
	 */
	ensureOpencodeRunning: (workspacePath?: string) => Promise<string | null>;
	createNewChat: (
		baseUrlOverride?: string,
		directoryOverride?: string,
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
	/** The current Main Chat session ID to send messages to (separate from selectedChatSessionId) */
	mainChatCurrentSessionId: string | null;
	setMainChatCurrentSessionId: (id: string | null) => void;
	/** Workspace path for the Main Chat assistant */
	mainChatWorkspacePath: string | null;
	setMainChatWorkspacePath: (path: string | null) => void;
}

export const AppContext = createContext<AppContextValue | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
	const [locale, setLocaleState] = useState<Locale>("de");
	const apps = useMemo(() => appRegistry.getAllApps(), []);
	const [activeAppId, setActiveAppId] = useState(() => apps[0]?.id ?? "");
	const activeApp = apps.find((app) => app.id === activeAppId) ?? apps[0];

	const [workspaceSessions, setWorkspaceSessions] = useState<
		WorkspaceSession[]
	>([]);
	const [selectedWorkspaceSessionId, setSelectedWorkspaceSessionId] =
		useState<string>("");
	// Chat history from disk (no running opencode needed)
	const [chatHistory, setChatHistory] = useState<ChatSession[]>([]);
	const chatHistoryRef = useRef<ChatSession[]>([]);
	// Keep ref in sync with state
	chatHistoryRef.current = chatHistory;
	// Live opencode sessions (requires running opencode instance)
	const [opencodeSessions, setOpencodeSessions] = useState<OpenCodeSession[]>(
		[],
	);
	const [selectedChatSessionId, setSelectedChatSessionId] = useState<string>(
		() => {
			// Allow mock session ID via URL parameter for testing
			if (typeof window !== "undefined") {
				const params = new URLSearchParams(window.location.search);
				const mockSession = params.get("mockSession");
				if (mockSession) {
					console.log("[Dev] Using mock session ID:", mockSession);
					return mockSession;
				}
			}
			return "";
		},
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
	// Main Chat state - when active, sessions app shows threaded view
	const [mainChatActive, setMainChatActive] = useState(false);
	const [mainChatAssistantName, setMainChatAssistantName] = useState<
		string | null
	>(null);
	// The current Main Chat session ID to send messages to (separate from selectedChatSessionId)
	const [mainChatCurrentSessionId, setMainChatCurrentSessionId] = useState<
		string | null
	>(null);
	const [mainChatWorkspacePath, setMainChatWorkspacePath] = useState<
		string | null
	>(null);

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

	// Get the selected chat from disk history (even if opencode isn't running)
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
		// Allow mock server override via URL parameter for testing
		// Usage: ?mockOpencode=http://localhost:7274
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

	const sessionEventSubscriptions = useRef(new Map<string, () => void>());

	useEffect(() => {
		const initialLocale = resolveStoredLocale();
		setLocaleState(initialLocale);
		document.documentElement.lang = initialLocale;
		void i18n.changeLanguage(initialLocale);

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

	// Refresh chat history from disk (no opencode needed)
	const refreshChatHistory = useCallback(async () => {
		try {
			// No limit - load all sessions from disk
			const history = await listChatHistory({ include_children: true });
			setChatHistory(history);

			// If no chat is selected but we have history, select the most recent one
			if (history.length > 0 && !mainChatActive) {
				setSelectedChatSessionId((current) => {
					if (current && history.some((s) => s.id === current)) return current;
					return history[0].id;
				});
			}
		} catch (err) {
			console.error("Failed to load chat history:", err);
		}
	}, [mainChatActive]);

	const refreshWorkspaceSessions = useCallback(async () => {
		try {
			// Load sessions and projects in parallel
			const [sessionsData, projectsData] = await Promise.all([
				listWorkspaceSessions().catch(() => [] as WorkspaceSession[]),
				listProjects().catch(() => [] as ProjectEntry[]),
			]);

			setWorkspaceSessions(sessionsData);
			setProjects(projectsData);

			if (sessionsData.length > 0) {
				setSelectedWorkspaceSessionId((current) => {
					// If no current selection, pick the first running session or first session
					if (!current) {
						const running = sessionsData.find((s) => s.status === "running");
						return running?.id || sessionsData[0].id;
					}
					// Check if current session exists
					const currentSession = sessionsData.find((s) => s.id === current);
					if (!currentSession) {
						return sessionsData[0].id;
					}
					return current;
				});
			}
		} catch (err) {
			console.error("Failed to load sessions:", err);
		}
	}, []);

	// Handle WebSocket events from all subscribed sessions
	const handleWsEvent = useCallback(
		(event: WsEvent) => {
			const sessionId =
				"session_id" in event ? (event.session_id as string) : undefined;

			if (event.type === "session_busy" && sessionId) {
				setSessionBusy(sessionId, true);
			} else if (event.type === "session_idle" && sessionId) {
				setSessionBusy(sessionId, false);
			}

			// Refresh workspace sessions on relevant events
			if (
				event.type === "session_updated" ||
				event.type === "session_deleted" ||
				event.type === "agent_connected" ||
				event.type === "agent_disconnected"
			) {
				refreshWorkspaceSessions();
			}
		},
		[refreshWorkspaceSessions, setSessionBusy],
	);

	// Start a new session for a specific project
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

	// Ensure opencode is running and return the base URL
	// This is called when the user wants to send a message
	// If workspacePath is provided, ensures a session for that specific workspace
	const ensureOpencodeRunning = useCallback(
		async (workspacePath?: string): Promise<string | null> => {
			try {
				let session: WorkspaceSession;

				if (workspacePath) {
					// Get or create a session for the specific workspace path
					// This handles multi-workspace resumption with LRU cap
					session = await getOrCreateSessionForWorkspace(workspacePath);
				} else {
					// If no workspace path provided, check if we have a running session
					if (selectedWorkspaceSession?.status === "running") {
						// Touch activity to prevent idle timeout
						touchSessionActivity(selectedWorkspaceSession.id).catch(() => {});
						return opencodeProxyBaseUrl(selectedWorkspaceSession.id);
					}
					// Start or resume a workspace session (default behavior)
					session = await getOrCreateWorkspaceSession();
				}

				// Refresh workspace sessions to get the updated state
				await refreshWorkspaceSessions();

				// Select this session
				setSelectedWorkspaceSessionId(session.id);

				// If session is already running, return immediately
				if (session.status === "running") {
					return opencodeProxyBaseUrl(session.id);
				}

				// Wait for it to be ready
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

		// Subscribe to new sessions
		for (const session of runningSessions) {
			if (sessionEventSubscriptions.current.has(session.id)) {
				continue;
			}
			// Subscribe to session events via WebSocket
			wsClient.subscribeSession(session.id);
			const unsubscribe = wsClient.onSessionEvent(session.id, handleWsEvent);
			sessionEventSubscriptions.current.set(session.id, unsubscribe);
		}

		// Unsubscribe from removed sessions
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
			// Select most recently updated session, but don't override history-only views.
			// Use ref to avoid chatHistory dependency causing re-renders
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
	}, [mainChatActive, opencodeBaseUrl, opencodeDirectory]);

	const createNewChat = useCallback(
		async (
			baseUrlOverride?: string,
			directoryOverride?: string,
		): Promise<OpenCodeSession | null> => {
			const baseUrl = baseUrlOverride || opencodeBaseUrl;
			if (!baseUrl) return null;
			try {
				const directory = directoryOverride || opencodeDirectory;
				const created = await createSession(baseUrl, undefined, undefined, {
					directory,
				});
				setOpencodeSessions((prev) => [created, ...prev]);
				setSelectedChatSessionId(created.id);
				// Refresh chat history to include the new session in the sidebar
				// Small delay to allow opencode to write the session to disk
				setTimeout(() => {
					refreshChatHistory();
				}, 500);
				return created;
			} catch (err) {
				console.error("Failed to create new chat session:", err);
				return null;
			}
		},
		[opencodeBaseUrl, opencodeDirectory, refreshChatHistory],
	);

	const createNewChatWithPersona = useCallback(
		async (
			persona: Persona,
			workspacePath?: string,
		): Promise<OpenCodeSession | null> => {
			try {
				// Resolve workspace path
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

				// Create a new workspace session with the selected persona
				const workspaceSession = await createWorkspaceSession({
					persona_id: persona.id,
					workspace_path: resolvedPath,
				});

				// Refresh workspace sessions to include the new one
				await refreshWorkspaceSessions();

				// Select the new workspace session
				setSelectedWorkspaceSessionId(workspaceSession.id);

				// Wait a moment for the workspace to be ready, then create a chat
				// The opencodeBaseUrl will update when selectedWorkspaceSession changes
				const baseUrl = opencodeProxyBaseUrl(workspaceSession.id);

				// Poll until the session is running
				let attempts = 0;
				const maxAttempts = 30;
				while (attempts < maxAttempts) {
					try {
						const created = await createSession(baseUrl, undefined, undefined, {
							directory: resolvedPath,
						});
						setOpencodeSessions((prev) => [created, ...prev]);
						setSelectedChatSessionId(created.id);
						// Refresh chat history to include the new session in the sidebar
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
				// If we deleted the selected session, select another one
				setSelectedChatSessionId((current) => {
					if (current !== sessionId) return current;
					const remaining = opencodeSessions.filter((s) => s.id !== sessionId);
					return remaining.length > 0 ? remaining[0].id : "";
				});
				// Refresh chat history to remove the deleted session from the sidebar
				setTimeout(() => {
					refreshChatHistory();
				}, 500);
				return true;
			} catch (err) {
				console.error("Failed to delete chat session:", err);
				return false;
			}
		},
		[opencodeBaseUrl, opencodeDirectory, opencodeSessions, refreshChatHistory],
	);

	const renameChatSession = useCallback(
		async (sessionId: string, title: string): Promise<boolean> => {
			try {
				// Check if this is a live session (has opencodeBaseUrl)
				if (opencodeBaseUrl) {
					// Try to update via opencode API first (for live sessions)
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
						// Also update the chat history state in case it's there too
						setChatHistory((prev) =>
							prev.map((s) => (s.id === sessionId ? { ...s, title } : s)),
						);
						return true;
					} catch {
						// Fall through to try the history API
					}
				}

				// Try the chat history API (for history-only sessions or if opencode update failed)
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

	const setLocale = useCallback((next: Locale) => {
		setLocaleState(next);
		document.documentElement.lang = next;
		void i18n.changeLanguage(next);
		try {
			window.localStorage.setItem("locale", next);
		} catch {
			// ignore storage failures
		}
	}, []);

	const resolveText = useCallback(
		(value: LocalizedText) => {
			if (typeof value === "string") return value;
			return locale === "en" ? value.en : value.de;
		},
		[locale],
	);

	const value = useMemo(
		() => ({
			apps,
			activeAppId,
			setActiveAppId,
			activeApp,
			locale,
			setLocale,
			resolveText,
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
		}),
		[
			apps,
			activeAppId,
			activeApp,
			locale,
			setLocale,
			resolveText,
			workspaceSessions,
			selectedWorkspaceSessionId,
			selectedWorkspaceSession,
			opencodeBaseUrl,
			opencodeDirectory,
			chatHistory,
			opencodeSessions,
			selectedChatSessionId,
			selectedChatSession,
			selectedChatFromHistory,
			busySessions,
			setSessionBusy,
			refreshWorkspaceSessions,
			refreshChatHistory,
			refreshOpencodeSessions,
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
			mainChatAssistantName,
			mainChatCurrentSessionId,
			mainChatWorkspacePath,
		],
	);

	return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}
