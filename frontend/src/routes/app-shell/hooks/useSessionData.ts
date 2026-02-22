import type {
	ChatSession,
	HstrySearchHit,
	ProjectLogo,
} from "@/lib/control-plane-client";
import { formatSessionDate, getTempIdFromSession } from "@/lib/session-utils";
import { useCallback, useMemo } from "react";
import type { SessionHierarchy, SessionsByProject } from "../SidebarSessions";
import type { WorkspaceDirectory } from "./useProjectActions";

export interface ProjectSummary {
	key: string;
	name: string;
	directory?: string;
	sessionCount: number;
	lastActive: number;
	logo?: ProjectLogo;
}

export interface SessionDataInput {
	chatHistory: ChatSession[];
	workspaceDirectories: WorkspaceDirectory[];
	locale: string;
	deferredSearch: string;
	pinnedSessions: Set<string>;
	pinnedProjects: string[];
	selectedProjectKey: string | null;
	projectSortBy: "date" | "name" | "sessions";
	projectSortAsc: boolean;
}

export interface SessionDataOutput {
	sessionHierarchy: SessionHierarchy;
	filteredSessions: ChatSession[];
	projectSummaries: ProjectSummary[];
	sessionsByProject: SessionsByProject[];
	selectedProjectLabel: string | null;
	projectKeyForSession: (
		session:
			| ChatSession
			| { directory?: string | null; projectID?: string | null },
	) => string;
	projectLabelForSession: (
		session:
			| ChatSession
			| { directory?: string | null; projectID?: string | null },
	) => string;
	sessionTitleHits: HstrySearchHit[];
}

export function useSessionData({
	chatHistory,
	workspaceDirectories,
	locale,
	deferredSearch,
	pinnedSessions,
	pinnedProjects,
	selectedProjectKey,
	projectSortBy,
	projectSortAsc,
}: SessionDataInput): SessionDataOutput {
	const dedupedChatHistory = useMemo(() => {
		const byId = new Map<string, ChatSession>();
		for (const session of chatHistory) {
			const existing = byId.get(session.id);
			if (!existing || session.updated_at >= existing.updated_at) {
				byId.set(session.id, session);
			}
		}
		return Array.from(byId.values());
	}, [chatHistory]);

	// Build hierarchical session structure
	const sessionHierarchy: SessionHierarchy = useMemo(() => {
		const parentSessions = dedupedChatHistory.filter((s) => !s.parent_id);
		const childSessionsByParent = new Map<string, ChatSession[]>();

		for (const session of dedupedChatHistory) {
			if (session.parent_id) {
				const children = childSessionsByParent.get(session.parent_id) || [];
				children.push(session);
				childSessionsByParent.set(session.parent_id, children);
			}
		}

		for (const [parentId, children] of childSessionsByParent) {
			childSessionsByParent.set(
				parentId,
				children.sort((a, b) => b.updated_at - a.updated_at),
			);
		}

		return { parentSessions, childSessionsByParent };
	}, [dedupedChatHistory]);

	const projectKeyForSession = useCallback(
		(
			session:
				| ChatSession
				| { directory?: string | null; projectID?: string | null },
		) => {
			if ("workspace_path" in session && session.workspace_path) {
				const normalized = session.workspace_path
					.replace(/\\/g, "/")
					.replace(/\/+$/, "");
				const parts = normalized.split("/").filter(Boolean);
				return parts[parts.length - 1] ?? session.workspace_path;
			}
			const directory = (
				"directory" in session ? session.directory : null
			)?.trim();
			if (directory) {
				const normalized = directory.replace(/\\/g, "/").replace(/\/+$/, "");
				const parts = normalized.split("/").filter(Boolean);
				return parts[parts.length - 1] ?? directory;
			}
			const projectId = (
				"projectID" in session ? session.projectID : null
			)?.trim();
			if (projectId) return projectId;
			return "workspace";
		},
		[],
	);

	const projectLabelForSession = useCallback(
		(
			session:
				| ChatSession
				| { directory?: string | null; projectID?: string | null },
		) => {
			if ("project_name" in session && session.project_name) {
				return session.project_name;
			}
			const directory = (
				"directory" in session ? session.directory : null
			)?.trim();
			if (directory) {
				const normalized = directory.replace(/\\/g, "/");
				const parts = normalized.split("/").filter(Boolean);
				return parts[parts.length - 1] ?? directory;
			}
			const projectId = (
				"projectID" in session ? session.projectID : null
			)?.trim();
			if (projectId) return projectId;
			return locale === "de" ? "Arbeitsbereich" : "Workspace";
		},
		[locale],
	);

	// Filter and sort sessions
	const filteredSessions = useMemo(() => {
		const searchLower = deferredSearch.toLowerCase().trim();
		let sessions = sessionHierarchy.parentSessions;

		if (selectedProjectKey) {
			sessions = sessions.filter(
				(session) => projectKeyForSession(session) === selectedProjectKey,
			);
		}

		if (searchLower) {
			sessions = sessions.filter((session) => {
				if (session.title?.toLowerCase().includes(searchLower)) return true;
				const tempId = getTempIdFromSession(session);
				if (tempId?.toLowerCase().includes(searchLower)) {
					return true;
				}
				if (session.updated_at) {
					const dateStr = formatSessionDate(session.updated_at);
					if (dateStr.toLowerCase().includes(searchLower)) return true;
				}
				if ("workspace_path" in session && session.workspace_path) {
					const dirName = session.workspace_path
						.split("/")
						.filter(Boolean)
						.pop();
					if (dirName?.toLowerCase().includes(searchLower)) return true;
				}
				return false;
			});
		}

		return [...sessions].sort((a, b) => {
			const aPinned = pinnedSessions.has(a.id);
			const bPinned = pinnedSessions.has(b.id);
			if (aPinned && !bPinned) return -1;
			if (!aPinned && bPinned) return 1;
			return b.updated_at - a.updated_at;
		});
	}, [
		sessionHierarchy.parentSessions,
		deferredSearch,
		pinnedSessions,
		projectKeyForSession,
		selectedProjectKey,
	]);

	const sessionTitleHits = useMemo(() => {
		const query = deferredSearch.trim().toLowerCase();
		if (!query) return [];

		return sessionHierarchy.parentSessions
			.filter((session) => {
				if (!session.title) return false;
				return session.title.toLowerCase().includes(query);
			})
			.map((session) => ({
				agent: "pi",
				source_path: `title:oc:${session.id}`,
				session_id: session.id,
				title: session.title ?? "New Session",
				timestamp: session.updated_at,
				match_type: "title",
				snippet: "Title match",
				workspace: session.workspace_path ?? undefined,
			}));
	}, [deferredSearch, sessionHierarchy.parentSessions]);

	const projectSummaries = useMemo(() => {
		const entries = new Map<string, ProjectSummary>();

		for (const directory of workspaceDirectories) {
			entries.set(directory.path, {
				key: directory.path,
				name: directory.name,
				directory: directory.path,
				sessionCount: 0,
				lastActive: 0,
				logo: directory.logo,
			});
		}

		for (const session of sessionHierarchy.parentSessions) {
			const key = projectKeyForSession(session);
			const name = projectLabelForSession(session);
			const lastActive = session.updated_at ?? 0;
			const existing = entries.get(key);
			if (existing) {
				existing.sessionCount += 1;
				if (lastActive > existing.lastActive) existing.lastActive = lastActive;
				if (session.workspace_path && !existing.directory?.startsWith("/")) {
					existing.directory = session.workspace_path;
				}
			} else {
				entries.set(key, {
					key,
					name,
					directory: session.workspace_path ?? undefined,
					sessionCount: 1,
					lastActive,
				});
			}
		}

		// Only add a fallback "Workspace" group in single-user/local mode
		// (no workspace directories). In multi-user mode, all sessions belong
		// to a real workspace directory -- orphaned sessions are ignored.
		if (!entries.has("workspace") && workspaceDirectories.length === 0) {
			entries.set("workspace", {
				key: "workspace",
				name: locale === "de" ? "Arbeitsbereich" : "Workspace",
				sessionCount: 0,
				lastActive: 0,
			});
		}

		return [...entries.values()].sort((a, b) => b.lastActive - a.lastActive);
	}, [
		projectKeyForSession,
		projectLabelForSession,
		sessionHierarchy.parentSessions,
		workspaceDirectories,
		locale,
	]);

	const selectedProjectLabel = useMemo(() => {
		if (!selectedProjectKey) return null;
		return (
			projectSummaries.find((project) => project.key === selectedProjectKey)
				?.name ?? selectedProjectKey
		);
	}, [projectSummaries, selectedProjectKey]);

	// Group filtered sessions by project
	const sessionsByProject: SessionsByProject[] = useMemo(() => {
		const groups = new Map<
			string,
			{
				key: string;
				name: string;
				directory?: string;
				sessions: ChatSession[];
				logo?: ProjectLogo;
			}
		>();

		// First, add all workspace directories (even those without sessions).
		// Key by the last path segment to match projectKeyForSession() which
		// extracts the basename from session.workspace_path.
		for (const directory of workspaceDirectories) {
			const normalized = directory.path.replace(/\\/g, "/").replace(/\/+$/, "");
			const parts = normalized.split("/").filter(Boolean);
			const key = parts[parts.length - 1] ?? directory.path;
			groups.set(key, {
				key,
				name: directory.name,
				directory: directory.path,
				sessions: [],
				logo: directory.logo,
			});
		}

		// Then, add sessions to their respective projects.
		// Skip orphaned sessions (key="workspace") when real workspace dirs exist
		// -- they have no valid path and can't be opened.
		for (const session of filteredSessions) {
			const key = projectKeyForSession(session);
			if (key === "workspace" && workspaceDirectories.length > 0) {
				continue;
			}
			const name = projectLabelForSession(session);
			const existing = groups.get(key);
			if (existing) {
				existing.sessions.push(session);
			} else {
				const projectInfo = projectSummaries.find((p) => p.key === key);
				groups.set(key, {
					key,
					name,
					directory: session.workspace_path ?? undefined,
					sessions: [session],
					logo: projectInfo?.logo,
				});
			}
		}

		return [...groups.values()].sort((a, b) => {
			const aPinned = pinnedProjects.includes(a.key);
			const bPinned = pinnedProjects.includes(b.key);
			if (aPinned && !bPinned) return -1;
			if (!aPinned && bPinned) return 1;

			let comparison = 0;
			if (projectSortBy === "date") {
				const aLatest = Math.max(
					...a.sessions.map((s) => s.updated_at ?? 0),
					0,
				);
				const bLatest = Math.max(
					...b.sessions.map((s) => s.updated_at ?? 0),
					0,
				);
				comparison = bLatest - aLatest;
			} else if (projectSortBy === "name") {
				comparison = a.name.localeCompare(b.name);
			} else if (projectSortBy === "sessions") {
				comparison = b.sessions.length - a.sessions.length;
			}

			return projectSortAsc ? -comparison : comparison;
		});
	}, [
		filteredSessions,
		workspaceDirectories,
		projectKeyForSession,
		projectLabelForSession,
		projectSummaries,
		pinnedProjects,
		projectSortBy,
		projectSortAsc,
	]);

	return {
		sessionHierarchy,
		filteredSessions,
		projectSummaries,
		sessionsByProject,
		selectedProjectLabel,
		projectKeyForSession,
		projectLabelForSession,
		sessionTitleHits,
	};
}
