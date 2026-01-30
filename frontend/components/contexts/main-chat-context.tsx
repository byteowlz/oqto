"use client";

import { getMainChatAssistant } from "@/lib/control-plane-client";
import {
	type ReactNode,
	createContext,
	startTransition,
	useCallback,
	useContext,
	useEffect,
	useMemo,
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
