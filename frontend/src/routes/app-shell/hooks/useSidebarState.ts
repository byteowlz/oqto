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

	// Expanded state for project groups in sidebar (persisted to localStorage)
	const [expandedProjects, setExpandedProjects] = useState<Set<string>>(
		() => {
			if (typeof window === "undefined") return new Set<string>();
			try {
				const stored = localStorage.getItem("oqto:expandedProjects");
				if (stored) {
					const parsed = JSON.parse(stored);
					if (Array.isArray(parsed)) return new Set<string>(parsed);
				}
			} catch {
				localStorage.removeItem("oqto:expandedProjects");
			}
			return new Set<string>();
		},
	);

	// Persist expanded projects to localStorage
	useEffect(() => {
		if (typeof window === "undefined") return;
		try {
			localStorage.setItem(
				"oqto:expandedProjects",
				JSON.stringify([...expandedProjects]),
			);
		} catch {
			// Ignore storage failures
		}
	}, [expandedProjects]);

	// Pinned sessions (persisted to localStorage)
	const [pinnedSessions, setPinnedSessions] = useState<Set<string>>(() => {
		if (typeof window === "undefined") return new Set();
		try {
			const stored = localStorage.getItem("oqto:pinnedSessions");
			return stored ? new Set(JSON.parse(stored)) : new Set();
		} catch {
			localStorage.removeItem("oqto:pinnedSessions");
			return new Set();
		}
	});

	// Persist pinned sessions to localStorage
	useEffect(() => {
		if (typeof window === "undefined") return;
		try {
			localStorage.setItem(
				"oqto:pinnedSessions",
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
			const stored = localStorage.getItem("oqto:pinnedProjects");
			return stored ? JSON.parse(stored) : [];
		} catch {
			localStorage.removeItem("oqto:pinnedProjects");
			return [];
		}
	});

	// Persist pinned projects to localStorage
	useEffect(() => {
		if (typeof window === "undefined") return;
		try {
			localStorage.setItem(
				"oqto:pinnedProjects",
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
