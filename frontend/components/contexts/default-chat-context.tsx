"use client";

import { getDefaultChatAssistant, listDefaultChatPiSessions } from "@/lib/api";
import { normalizeWorkspacePath } from "@/lib/session-utils";
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

export interface DefaultChatContextValue {
	/** Default chat state - when active, shows threaded view of all default chat sessions */
	defaultChatActive: boolean;
	setDefaultChatActive: (active: boolean) => void;
	defaultChatAssistantName: string | null;
	setDefaultChatAssistantName: (name: string | null) => void;
	/** The current default chat session ID to send messages to */
	defaultChatCurrentSessionId: string | null;
	setDefaultChatCurrentSessionId: (id: string | null) => void;
	/** Workspace path for the default chat assistant */
	defaultChatWorkspacePath: string | null;
	setDefaultChatWorkspacePath: (path: string | null) => void;
	/** Trigger that increments when default chat session has activity (message sent) */
	sessionActivityTrigger: number;
	/** Notify that default chat session has activity (increments trigger) */
	notifySessionActivity: () => void;
	/** Target message ID to scroll to after navigation (from search) */
	scrollToMessageId: string | null;
	setScrollToMessageId: (id: string | null) => void;
}

// Default no-op functions for HMR resilience
const noop = () => {};

const defaultDefaultChatContext: DefaultChatContextValue = {
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

const DefaultChatContext = createContext<DefaultChatContextValue>(
	defaultDefaultChatContext,
);

export function DefaultChatProvider({ children }: { children: ReactNode }) {
	// Default chat state - restore from localStorage, default to default chat if no last session
	const [defaultChatActive, setDefaultChatActiveRaw] = useState(() => {
		if (typeof window !== "undefined") {
			try {
				const lastSessionId = localStorage.getItem("octo:lastChatSessionId");
				const lastDefaultChatActive = localStorage.getItem(
					"octo:lastDefaultChatActive",
				);
				// If we have a stored preference, use it
				if (lastDefaultChatActive !== null) {
					return lastDefaultChatActive === "true";
				}
				// If we have a last session ID, default to the session
				if (lastSessionId) {
					return false;
				}
			} catch {
				localStorage.removeItem("octo:lastChatSessionId");
				localStorage.removeItem("octo:lastDefaultChatActive");
			}
		}
		// Default to default chat if nothing stored
		return true;
	});

	const setDefaultChatActive = useCallback(
		(value: boolean | ((prev: boolean) => boolean)) => {
			startTransition(() => {
				setDefaultChatActiveRaw((prev) => {
					const newValue = typeof value === "function" ? value(prev) : value;
					// Persist to localStorage
					if (typeof window !== "undefined") {
						try {
							localStorage.setItem(
								"octo:lastDefaultChatActive",
								String(newValue),
							);
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

	const [defaultChatAssistantName, setDefaultChatAssistantName] = useState<
		string | null
	>(null);

	// Default chat session ID - restore from localStorage for instant load
	const [defaultChatCurrentSessionId, setDefaultChatCurrentSessionIdRaw] =
		useState<string | null>(() => {
			if (typeof window !== "undefined") {
				try {
					return localStorage.getItem("octo:defaultChatCurrentSessionId");
				} catch {
					localStorage.removeItem("octo:defaultChatCurrentSessionId");
				}
			}
			return null;
		});

	// Track if we've attempted to restore the session
	const sessionRestoreAttempted = useRef(false);

	// Wrap setter to persist to localStorage
	const setDefaultChatCurrentSessionId = useCallback((id: string | null) => {
		setDefaultChatCurrentSessionIdRaw(id);
		if (typeof window !== "undefined") {
			try {
				if (id) {
					localStorage.setItem("octo:defaultChatCurrentSessionId", id);
				} else {
					localStorage.removeItem("octo:defaultChatCurrentSessionId");
				}
			} catch {
				localStorage.removeItem("octo:defaultChatCurrentSessionId");
			}
		}
	}, []);

	// Default chat workspace path - cached to localStorage for instant load
	const [defaultChatWorkspacePath, setDefaultChatWorkspacePathRaw] = useState<
		string | null
	>(null);

	// Wrap setter to also cache to localStorage
	const setDefaultChatWorkspacePath = useCallback((path: string | null) => {
		const normalized = normalizeWorkspacePath(path);
		setDefaultChatWorkspacePathRaw(normalized);
		if (typeof window !== "undefined") {
			try {
				if (normalized) {
					localStorage.setItem("defaultChatWorkspacePath", normalized);
				} else {
					localStorage.removeItem("defaultChatWorkspacePath");
				}
			} catch {
				localStorage.removeItem("defaultChatWorkspacePath");
			}
		}
	}, []);

	const [scrollToMessageId, setScrollToMessageId] = useState<string | null>(
		null,
	);

	// Keep default chat info available even when not active.
	useEffect(() => {
		// Try to load from cache first for instant display
		if (typeof window !== "undefined") {
			try {
				const cached = normalizeWorkspacePath(
					localStorage.getItem("defaultChatWorkspacePath"),
				);
				if (cached) setDefaultChatWorkspacePathRaw(cached);
			} catch {
				localStorage.removeItem("defaultChatWorkspacePath");
			}
		}

		let cancelled = false;
		getDefaultChatAssistant("default")
			.then((info) => {
				if (!cancelled) {
					setDefaultChatWorkspacePath(info.path);
					setDefaultChatAssistantName(info.name);
				}
			})
			.catch((err) => {
				console.debug("[DefaultChat] No default chat configured:", err.message);
				setDefaultChatWorkspacePath(null);
			});
		return () => {
			cancelled = true;
		};
	}, [setDefaultChatWorkspacePath]);

	// Restore last session or fetch the most recent one when default chat becomes active
	useEffect(() => {
		if (!defaultChatActive || sessionRestoreAttempted.current) {
			return;
		}
		sessionRestoreAttempted.current = true;

		// If we already have a session ID from localStorage, we're done
		if (defaultChatCurrentSessionId) {
			return;
		}

		// No stored session - fetch the most recent session from API
		let cancelled = false;
		listDefaultChatPiSessions()
			.then((sessions) => {
				if (cancelled) return;
				if (sessions.length > 0) {
					// Sort by modified_at descending (it's a timestamp number) and pick the most recent
					const sorted = [...sessions].sort(
						(a, b) => (b.modified_at || 0) - (a.modified_at || 0),
					);
					const mostRecent = sorted[0];
					console.log(
						"[DefaultChat] Restoring most recent session:",
						mostRecent.id,
					);
					setDefaultChatCurrentSessionId(mostRecent.id);
				}
				// If no sessions exist, leave it null - user will start a new session when they send a message
			})
			.catch((err) => {
				console.debug("[DefaultChat] Failed to list sessions:", err.message);
			});

		return () => {
			cancelled = true;
		};
	}, [
		defaultChatActive,
		defaultChatCurrentSessionId,
		setDefaultChatCurrentSessionId,
	]);

	// Trigger for Default Chat session activity (message sent)
	const [sessionActivityTrigger, setSessionActivityTrigger] = useState(0);

	// Notify that Default Chat session has activity
	const notifySessionActivity = useCallback(() => {
		setSessionActivityTrigger((prev) => prev + 1);
	}, []);

	const value = useMemo(
		() => ({
			defaultChatActive,
			setDefaultChatActive,
			defaultChatAssistantName,
			setDefaultChatAssistantName,
			defaultChatCurrentSessionId,
			setDefaultChatCurrentSessionId,
			defaultChatWorkspacePath,
			setDefaultChatWorkspacePath,
			sessionActivityTrigger,
			notifySessionActivity,
			scrollToMessageId,
			setScrollToMessageId,
		}),
		[
			defaultChatActive,
			setDefaultChatActive,
			defaultChatAssistantName,
			defaultChatCurrentSessionId,
			setDefaultChatCurrentSessionId,
			defaultChatWorkspacePath,
			setDefaultChatWorkspacePath,
			sessionActivityTrigger,
			notifySessionActivity,
			scrollToMessageId,
		],
	);

	return (
		<DefaultChatContext.Provider value={value}>
			{children}
		</DefaultChatContext.Provider>
	);
}

export function useDefaultChatContext() {
	return useContext(DefaultChatContext);
}

// Convenience hook that matches the original useDefaultChat API
export function useDefaultChat() {
	const {
		defaultChatActive,
		setDefaultChatActive,
		defaultChatAssistantName,
		setDefaultChatAssistantName,
		defaultChatCurrentSessionId,
		setDefaultChatCurrentSessionId,
		defaultChatWorkspacePath,
		setDefaultChatWorkspacePath,
	} = useDefaultChatContext();
	return {
		defaultChatActive,
		setDefaultChatActive,
		defaultChatAssistantName,
		setDefaultChatAssistantName,
		defaultChatCurrentSessionId,
		setDefaultChatCurrentSessionId,
		defaultChatWorkspacePath,
		setDefaultChatWorkspacePath,
	};
}
