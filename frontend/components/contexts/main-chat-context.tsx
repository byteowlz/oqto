"use client";

import { getMainChatAssistant, listMainChatPiSessions } from "@/lib/control-plane-client";
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

export interface MainChatContextValue {
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
	/** Trigger to create a new Main Chat session - increment to trigger */
	mainChatNewSessionTrigger: number;
	/** Request a new Main Chat session (increments trigger) */
	requestNewMainChatSession: () => void;
	/** Trigger that increments when Main Chat session has activity (message sent) */
	mainChatSessionActivityTrigger: number;
	/** Notify that Main Chat session has activity (increments trigger) */
	notifyMainChatSessionActivity: () => void;
	/** Target message ID to scroll to after navigation (from search) */
	scrollToMessageId: string | null;
	setScrollToMessageId: (id: string | null) => void;
}

// Default no-op functions for HMR resilience
const noop = () => {};

const defaultMainChatContext: MainChatContextValue = {
	mainChatActive: false,
	setMainChatActive: noop,
	mainChatAssistantName: null,
	setMainChatAssistantName: noop,
	mainChatCurrentSessionId: null,
	setMainChatCurrentSessionId: noop,
	mainChatWorkspacePath: null,
	setMainChatWorkspacePath: noop,
	mainChatNewSessionTrigger: 0,
	requestNewMainChatSession: noop,
	mainChatSessionActivityTrigger: 0,
	notifyMainChatSessionActivity: noop,
	scrollToMessageId: null,
	setScrollToMessageId: noop,
};

const MainChatContext = createContext<MainChatContextValue>(
	defaultMainChatContext,
);

export function MainChatProvider({ children }: { children: ReactNode }) {
	// Main Chat state - restore from localStorage, default to main chat if no last session
	const [mainChatActive, setMainChatActiveRaw] = useState(() => {
		if (typeof window !== "undefined") {
			try {
				const lastSessionId = localStorage.getItem("octo:lastChatSessionId");
				const lastMainChatActive = localStorage.getItem(
					"octo:lastMainChatActive",
				);
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
	
	// Main chat session ID - restore from localStorage for instant load
	const [mainChatCurrentSessionId, setMainChatCurrentSessionIdRaw] = useState<
		string | null
	>(() => {
		if (typeof window !== "undefined") {
			try {
				return localStorage.getItem("octo:mainChatCurrentSessionId");
			} catch {
				// Ignore localStorage errors
			}
		}
		return null;
	});
	
	// Track if we've attempted to restore the session
	const sessionRestoreAttempted = useRef(false);
	
	// Wrap setter to persist to localStorage
	const setMainChatCurrentSessionId = useCallback((id: string | null) => {
		setMainChatCurrentSessionIdRaw(id);
		if (typeof window !== "undefined") {
			try {
				if (id) {
					localStorage.setItem("octo:mainChatCurrentSessionId", id);
				} else {
					localStorage.removeItem("octo:mainChatCurrentSessionId");
				}
			} catch {
				// Ignore localStorage errors
			}
		}
	}, []);

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

	// Keep main chat info available even when not active.
	useEffect(() => {
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

		let cancelled = false;
		getMainChatAssistant("default")
			.then((info) => {
				if (!cancelled) {
					setMainChatWorkspacePath(info.path);
					setMainChatAssistantName(info.name);
				}
			})
			.catch((err) => {
				console.debug("[MainChat] No main chat configured:", err.message);
				setMainChatWorkspacePath(null);
			});
		return () => {
			cancelled = true;
		};
	}, [setMainChatWorkspacePath, setMainChatAssistantName]);

	// Restore last session or fetch the most recent one when main chat becomes active
	useEffect(() => {
		if (!mainChatActive || sessionRestoreAttempted.current) {
			return;
		}
		sessionRestoreAttempted.current = true;

		// If we already have a session ID from localStorage, we're done
		if (mainChatCurrentSessionId) {
			return;
		}

		// No stored session - fetch the most recent session from API
		let cancelled = false;
		listMainChatPiSessions()
			.then((sessions) => {
				if (cancelled) return;
				if (sessions.length > 0) {
					// Sort by modified_at descending (it's a timestamp number) and pick the most recent
					const sorted = [...sessions].sort((a, b) => (b.modified_at || 0) - (a.modified_at || 0));
					const mostRecent = sorted[0];
					console.log("[MainChat] Restoring most recent session:", mostRecent.id);
					setMainChatCurrentSessionId(mostRecent.id);
				}
				// If no sessions exist, leave it null - user will start a new session when they send a message
			})
			.catch((err) => {
				console.debug("[MainChat] Failed to list sessions:", err.message);
			});

		return () => {
			cancelled = true;
		};
	}, [mainChatActive, mainChatCurrentSessionId, setMainChatCurrentSessionId]);

	// Trigger for creating a new Main Chat session
	const [mainChatNewSessionTrigger, setMainChatNewSessionTrigger] = useState(0);

	// Request a new Main Chat session by incrementing the trigger
	const requestNewMainChatSession = useCallback(() => {
		setMainChatNewSessionTrigger((prev) => prev + 1);
	}, []);

	// Trigger for Main Chat session activity (message sent)
	const [mainChatSessionActivityTrigger, setMainChatSessionActivityTrigger] =
		useState(0);

	// Notify that Main Chat session has activity
	const notifyMainChatSessionActivity = useCallback(() => {
		setMainChatSessionActivityTrigger((prev) => prev + 1);
	}, []);

	const value = useMemo(
		() => ({
			mainChatActive,
			setMainChatActive,
			mainChatAssistantName,
			setMainChatAssistantName,
			mainChatCurrentSessionId,
			setMainChatCurrentSessionId,
			mainChatWorkspacePath,
			setMainChatWorkspacePath,
			mainChatNewSessionTrigger,
			requestNewMainChatSession,
			mainChatSessionActivityTrigger,
			notifyMainChatSessionActivity,
			scrollToMessageId,
			setScrollToMessageId,
		}),
		[
			mainChatActive,
			setMainChatActive,
			mainChatAssistantName,
			mainChatCurrentSessionId,
			mainChatWorkspacePath,
			setMainChatWorkspacePath,
			mainChatNewSessionTrigger,
			requestNewMainChatSession,
			mainChatSessionActivityTrigger,
			notifyMainChatSessionActivity,
			scrollToMessageId,
		],
	);

	return (
		<MainChatContext.Provider value={value}>
			{children}
		</MainChatContext.Provider>
	);
}

export function useMainChatContext() {
	return useContext(MainChatContext);
}

// Convenience hook that matches the original useMainChat API
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
	} = useMainChatContext();
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
