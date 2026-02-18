"use client";

import {
	type ProjectEntry,
	type WorkspaceSession,
	deleteWorkspaceSession,
	getOrCreateSessionForWorkspace,
	getOrCreateWorkspaceSession,
	listProjects,
	listWorkspaceSessions,
	stopWorkspaceSession,
	touchSessionActivity,
	upgradeWorkspaceSession,
} from "@/lib/control-plane-client";
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

export interface WorkspaceContextValue {
	workspaceSessions: WorkspaceSession[];
	selectedWorkspaceSessionId: string;
	setSelectedWorkspaceSessionId: (id: string) => void;
	selectedWorkspaceSession: WorkspaceSession | undefined;
	selectedWorkspaceOverviewPath: string | null;
	setSelectedWorkspaceOverviewPath: (path: string | null) => void;
	/** Available projects (directories in workspace_dir) */
	projects: ProjectEntry[];
	/** Start a new session for a project */
	startProjectSession: (
		projectPath: string,
	) => Promise<WorkspaceSession | null>;
	projectDefaultAgents: Record<string, string>;
	setProjectDefaultAgents: Dispatch<SetStateAction<Record<string, string>>>;
	refreshWorkspaceSessions: () => Promise<void>;
	/** Ensure the workspace runner session exists (no legacy assumptions). */
	ensureWorkspaceRunning: (
		workspacePath?: string,
	) => Promise<WorkspaceSession | null>;
	stopWorkspaceSession: (sessionId: string) => Promise<boolean>;
	deleteWorkspaceSession: (sessionId: string) => Promise<boolean>;
	upgradeWorkspaceSession: (sessionId: string) => Promise<boolean>;
	/** Set of workspace session IDs that are currently busy (agent working) */
	busyWorkspaceSessions: Set<string>;
	/** Mark a workspace session as busy or idle */
	setWorkspaceSessionBusy: (sessionId: string, busy: boolean) => void;
}

// Default no-op functions for HMR resilience
const noop = () => {};
const asyncNoop = async () => null;
const asyncNoopVoid = async () => {};
const asyncNoopBool = async () => false;

const defaultWorkspaceContext: WorkspaceContextValue = {
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
	busyWorkspaceSessions: new Set(),
	setWorkspaceSessionBusy: noop,
};

const WorkspaceContext = createContext<WorkspaceContextValue>(
	defaultWorkspaceContext,
);

export function WorkspaceProvider({ children }: { children: ReactNode }) {
	const [workspaceSessions, setWorkspaceSessions] = useState<
		WorkspaceSession[]
	>([]);
	const workspaceSessionsRef = useRef<WorkspaceSession[]>([]);
	workspaceSessionsRef.current = workspaceSessions;

	const [selectedWorkspaceSessionId, setSelectedWorkspaceSessionId] =
		useState<string>("");
	const [selectedWorkspaceOverviewPath, setSelectedWorkspaceOverviewPath] =
		useState<string | null>(null);

	// Available projects
	const [projects, setProjects] = useState<ProjectEntry[]>([]);
	const [projectDefaultAgents, setProjectDefaultAgents] = useState<
		Record<string, string>
	>(() => {
		if (typeof window === "undefined") return {};
		try {
			const stored = localStorage.getItem("oqto:projectDefaultAgents");
			return stored ? JSON.parse(stored) : {};
		} catch {
			localStorage.removeItem("oqto:projectDefaultAgents");
			return {};
		}
	});

	// Track which workspace sessions are currently busy (agent working)
	const [busyWorkspaceSessions, setBusyWorkspaceSessions] = useState<
		Set<string>
	>(new Set());

	const setWorkspaceSessionBusy = useCallback(
		(sessionId: string, busy: boolean) => {
			setBusyWorkspaceSessions((prev) => {
				const next = new Set(prev);
				if (busy) {
					next.add(sessionId);
				} else {
					next.delete(sessionId);
				}
				return next;
			});
		},
		[],
	);

	const sessionEventSubscriptions = useRef(new Map<string, () => void>());

	const selectedWorkspaceSession = useMemo(() => {
		if (!selectedWorkspaceSessionId) return undefined;
		return workspaceSessions.find(
			(session) => session.id === selectedWorkspaceSessionId,
		);
	}, [selectedWorkspaceSessionId, workspaceSessions]);

	const _ = selectedWorkspaceSession;

	// Restore selected session from localStorage
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

	// Persist selected session to localStorage
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

	// Persist project default agents to localStorage
	useEffect(() => {
		if (typeof window === "undefined") return;
		try {
			localStorage.setItem(
				"oqto:projectDefaultAgents",
				JSON.stringify(projectDefaultAgents),
			);
		} catch {
			// Ignore storage failures.
		}
	}, [projectDefaultAgents]);

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
							const running = sessionsData.find((s) => s.status === "running");
							return running?.id || sessionsData[0].id;
						}
						const currentSession = sessionsData.find((s) => s.id === current);
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
	const refreshTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
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
				setWorkspaceSessionBusy(sessionId, true);
			} else if (event.type === "session_idle" && sessionId) {
				setWorkspaceSessionBusy(sessionId, false);
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
		[debouncedRefreshWorkspaceSessions, setWorkspaceSessionBusy],
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

	const ensureWorkspaceRunning = useCallback(
		async (workspacePath?: string): Promise<WorkspaceSession | null> => {
			try {
				let session: WorkspaceSession;
				if (workspacePath) {
					session = await getOrCreateSessionForWorkspace(workspacePath);
				} else {
					if (selectedWorkspaceSession?.status === "running") {
						touchSessionActivity(selectedWorkspaceSession.id).catch(() => {});
						return selectedWorkspaceSession;
					}
					session = await getOrCreateWorkspaceSession();
				}

				await refreshWorkspaceSessions();
				setSelectedWorkspaceSessionId(session.id);
				return session;
			} catch (err) {
				console.error("Failed to ensure workspace is running:", err);
				return null;
			}
		},
		[selectedWorkspaceSession, refreshWorkspaceSessions],
	);

	// Initial load
	useEffect(() => {
		refreshWorkspaceSessions();
	}, [refreshWorkspaceSessions]);

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

	// Poll while starting
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

	const value = useMemo(
		() => ({
			workspaceSessions,
			selectedWorkspaceSessionId,
			setSelectedWorkspaceSessionId,
			selectedWorkspaceSession,
			selectedWorkspaceOverviewPath,
			setSelectedWorkspaceOverviewPath,
			projects,
			startProjectSession,
			projectDefaultAgents,
			setProjectDefaultAgents,
			refreshWorkspaceSessions,
			ensureWorkspaceRunning,
			stopWorkspaceSession: handleStopWorkspaceSession,
			deleteWorkspaceSession: handleDeleteWorkspaceSession,
			upgradeWorkspaceSession: handleUpgradeWorkspaceSession,
			busyWorkspaceSessions,
			setWorkspaceSessionBusy,
		}),
		[
			workspaceSessions,
			selectedWorkspaceSessionId,
			selectedWorkspaceSession,
			selectedWorkspaceOverviewPath,
			projects,
			startProjectSession,
			projectDefaultAgents,
			refreshWorkspaceSessions,
			ensureWorkspaceRunning,
			handleStopWorkspaceSession,
			handleDeleteWorkspaceSession,
			handleUpgradeWorkspaceSession,
			busyWorkspaceSessions,
			setWorkspaceSessionBusy,
		],
	);

	return (
		<WorkspaceContext.Provider value={value}>
			{children}
		</WorkspaceContext.Provider>
	);
}

export function useWorkspaceContext() {
	return useContext(WorkspaceContext);
}

// Convenience hook that matches the original useWorkspaceSessions API
export function useWorkspaceSessions() {
	const {
		workspaceSessions,
		selectedWorkspaceSessionId,
		setSelectedWorkspaceSessionId,
		selectedWorkspaceSession,
		refreshWorkspaceSessions,
	} = useWorkspaceContext();
	return {
		workspaceSessions,
		selectedWorkspaceSessionId,
		setSelectedWorkspaceSessionId,
		selectedWorkspaceSession,
		refreshWorkspaceSessions,
	};
}
