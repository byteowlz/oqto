import { useLocalStorage } from "@/hooks/use-local-storage";
import { useCallback, useState } from "react";

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

function parseStringArray(raw: string): string[] {
	const parsed = JSON.parse(raw);
	if (!Array.isArray(parsed)) {
		throw new Error("Expected array");
	}
	return parsed.filter((value): value is string => typeof value === "string");
}

export function useSidebarState(): SidebarState {
	const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
	const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

	// Expanded state for parent sessions in sidebar
	const [expandedSessions, setExpandedSessions] = useState<Set<string>>(
		new Set(),
	);

	const [expandedProjects, setExpandedProjects] = useLocalStorage<Set<string>>(
		"oqto:expandedProjects",
		() => new Set<string>(),
		{
			deserialize: (raw) => new Set(parseStringArray(raw)),
			serialize: (value) => JSON.stringify([...value]),
			onError: () => {
				if (typeof window === "undefined") return;
				window.localStorage.removeItem("oqto:expandedProjects");
			},
		},
	);

	const [pinnedSessions, setPinnedSessions] = useLocalStorage<Set<string>>(
		"oqto:pinnedSessions",
		() => new Set<string>(),
		{
			deserialize: (raw) => new Set(parseStringArray(raw)),
			serialize: (value) => JSON.stringify([...value]),
			onError: () => {
				if (typeof window === "undefined") return;
				window.localStorage.removeItem("oqto:pinnedSessions");
			},
		},
	);

	const [pinnedProjects, setPinnedProjects] = useLocalStorage<string[]>(
		"oqto:pinnedProjects",
		[],
		{
			deserialize: parseStringArray,
			onError: () => {
				if (typeof window === "undefined") return;
				window.localStorage.removeItem("oqto:pinnedProjects");
			},
		},
	);

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

	const toggleProjectExpanded = useCallback(
		(projectKey: string) => {
			setExpandedProjects((prev) => {
				const next = new Set(prev);
				if (next.has(projectKey)) {
					next.delete(projectKey);
				} else {
					next.add(projectKey);
				}
				return next;
			});
		},
		[setExpandedProjects],
	);

	const togglePinSession = useCallback(
		(sessionId: string) => {
			setPinnedSessions((prev) => {
				const next = new Set(prev);
				if (next.has(sessionId)) {
					next.delete(sessionId);
				} else {
					next.add(sessionId);
				}
				return next;
			});
		},
		[setPinnedSessions],
	);

	const togglePinProject = useCallback(
		(projectKey: string) => {
			setPinnedProjects((prev) => {
				if (prev.includes(projectKey)) {
					return prev.filter((k) => k !== projectKey);
				}
				return [...prev, projectKey];
			});
		},
		[setPinnedProjects],
	);

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
