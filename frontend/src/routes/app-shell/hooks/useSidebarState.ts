import { useCallback, useEffect, useState } from "react";

export interface SidebarState {
	sidebarCollapsed: boolean;
	setSidebarCollapsed: React.Dispatch<React.SetStateAction<boolean>>;
	mobileMenuOpen: boolean;
	setMobileMenuOpen: React.Dispatch<React.SetStateAction<boolean>>;
	expandedSessions: Set<string>;
	toggleSessionExpanded: (sessionId: string) => void;
	expandedProjects: Set<string>;
	toggleProjectExpanded: (projectKey: string) => void;
	pinnedSessions: Set<string>;
	togglePinSession: (sessionId: string) => void;
	pinnedProjects: string[];
	togglePinProject: (projectKey: string) => void;
}

export function useSidebarState(): SidebarState {
	const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
	const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

	// Expanded state for parent sessions in sidebar
	const [expandedSessions, setExpandedSessions] = useState<Set<string>>(
		new Set(),
	);

	// Expanded state for project groups in sidebar (default: all expanded)
	const [expandedProjects, setExpandedProjects] = useState<Set<string>>(
		() => new Set(["__all__"]),
	);

	// Pinned sessions (persisted to localStorage)
	const [pinnedSessions, setPinnedSessions] = useState<Set<string>>(() => {
		if (typeof window === "undefined") return new Set();
		try {
			const stored = localStorage.getItem("octo:pinnedSessions");
			return stored ? new Set(JSON.parse(stored)) : new Set();
		} catch {
			localStorage.removeItem("octo:pinnedSessions");
			return new Set();
		}
	});

	// Persist pinned sessions to localStorage
	useEffect(() => {
		if (typeof window === "undefined") return;
		try {
			localStorage.setItem(
				"octo:pinnedSessions",
				JSON.stringify([...pinnedSessions]),
			);
		} catch {
			// Ignore storage failures (private mode, denied access).
		}
	}, [pinnedSessions]);

	// Pinned projects for filter bar (persisted to localStorage)
	const [pinnedProjects, setPinnedProjects] = useState<string[]>(() => {
		if (typeof window === "undefined") return [];
		try {
			const stored = localStorage.getItem("octo:pinnedProjects");
			return stored ? JSON.parse(stored) : [];
		} catch {
			localStorage.removeItem("octo:pinnedProjects");
			return [];
		}
	});

	// Persist pinned projects to localStorage
	useEffect(() => {
		if (typeof window === "undefined") return;
		try {
			localStorage.setItem(
				"octo:pinnedProjects",
				JSON.stringify(pinnedProjects),
			);
		} catch {
			// Ignore storage failures (private mode, denied access).
		}
	}, [pinnedProjects]);

	const toggleSessionExpanded = useCallback((sessionId: string) => {
		setExpandedSessions((prev) => {
			const next = new Set(prev);
			if (next.has(sessionId)) {
				next.delete(sessionId);
			} else {
				next.add(sessionId);
			}
			return next;
		});
	}, []);

	const toggleProjectExpanded = useCallback((projectKey: string) => {
		setExpandedProjects((prev) => {
			const next = new Set(prev);
			if (next.has(projectKey)) {
				next.delete(projectKey);
			} else {
				next.add(projectKey);
			}
			return next;
		});
	}, []);

	const togglePinSession = useCallback((sessionId: string) => {
		setPinnedSessions((prev) => {
			const next = new Set(prev);
			if (next.has(sessionId)) {
				next.delete(sessionId);
			} else {
				next.add(sessionId);
			}
			return next;
		});
	}, []);

	const togglePinProject = useCallback((projectKey: string) => {
		setPinnedProjects((prev) => {
			if (prev.includes(projectKey)) {
				return prev.filter((k) => k !== projectKey);
			}
			return [...prev, projectKey];
		});
	}, []);

	return {
		sidebarCollapsed,
		setSidebarCollapsed,
		mobileMenuOpen,
		setMobileMenuOpen,
		expandedSessions,
		toggleSessionExpanded,
		expandedProjects,
		toggleProjectExpanded,
		pinnedSessions,
		togglePinSession,
		pinnedProjects,
		togglePinProject,
	};
}
