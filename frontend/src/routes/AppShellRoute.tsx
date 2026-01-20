import { AgentPicker } from "@/components/agent-picker";
import { AppProvider } from "@/components/app-context";
import { CommandPalette } from "@/components/command-palette";
import { MainChatEntry } from "@/components/main-chat";
import { StatusBar } from "@/components/status-bar";
import {
	type AgentFilter,
	type SearchMode,
	SearchResults,
} from "@/components/search";
import {
	AlertDialog,
	AlertDialogAction,
	AlertDialogCancel,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { useApp } from "@/hooks/use-app";
import { useCommandPalette } from "@/hooks/use-command-palette";
import {
	type CassSearchHit,
	type ChatSession,
	type CreateProjectFromTemplateRequest,
	type ListProjectTemplatesResponse,
	type Persona,
	type ProjectLogo,
	type ProjectTemplateEntry,
	createProjectFromTemplate,
	getProjectLogoUrl,
	getSettingsValues,
	listProjectTemplates,
	listWorkspaceDirectories,
} from "@/lib/control-plane-client";
import { type OpenCodeAgent, fetchAgents } from "@/lib/opencode-client";
import { formatSessionDate, generateReadableId } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	ArrowDown,
	ArrowUp,
	ArrowUpDown,
	Bot,
	ChevronDown,
	ChevronRight,
	Clock,
	Copy,
	FolderKanban,
	FolderPlus,
	Globe2,
	LayoutDashboard,
	Loader2,
	Menu,
	MessageSquare,
	MoonStar,
	PanelLeftClose,
	PanelRightClose,
	Pencil,
	Pin,
	Plus,
	Search,
	Settings,
	Shield,
	SunMedium,
	Trash2,
	X,
} from "lucide-react";
import { useTheme } from "next-themes";
import {
	memo,
	useCallback,
	useDeferredValue,
	useEffect,
	useMemo,
	useState,
} from "react";
import { useLocation, useNavigate } from "react-router-dom";
import "@/apps";
import {
	listMainChatPiSessions,
} from "@/features/main-chat/api";
import { useMainChatNavigation } from "@/features/main-chat/hooks/useMainChatNavigation";

const AppShell = memo(function AppShell() {
	const {
		apps,
		activeAppId,
		setActiveAppId,
		activeApp,
		locale,
		setLocale,
		resolveText,
		chatHistory,
		opencodeSessions,
		selectedChatSessionId,
		setSelectedChatSessionId,
		selectedChatSession,
		selectedChatFromHistory,
		selectedWorkspaceSession,
		opencodeBaseUrl,
		opencodeDirectory,
		ensureOpencodeRunning,
		createOptimisticChatSession,
		clearOptimisticChatSession,
		createNewChat,
		createNewChatWithPersona,
		deleteChatSession,
		renameChatSession,
		busySessions,
		workspaceSessions,
		projectDefaultAgents,
		setProjectDefaultAgents,
		mainChatActive,
		setMainChatActive,
		mainChatAssistantName,
		setMainChatAssistantName,
		mainChatCurrentSessionId,
		setMainChatCurrentSessionId,
		setMainChatWorkspacePath,
		setScrollToMessageId,
		requestNewMainChatSession,
	} = useApp();
	const location = useLocation();
	const navigate = useNavigate();
	const { theme, setTheme, resolvedTheme } = useTheme();
	const [mounted, setMounted] = useState(false);
	const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
	const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

	const matchedAppId = useMemo(() => {
		const path = location.pathname;
		const matchedApp = apps.find((app) =>
			app.routes?.some(
				(route) => path === route || path.startsWith(`${route}/`),
			),
		);
		return matchedApp?.id;
	}, [apps, location.pathname]);

	const sessionsRoute = useMemo(
		() => apps.find((app) => app.id === "sessions")?.routes?.[0],
		[apps],
	);

	const virtualApps = useMemo(
		() => new Set(["dashboard", "settings", "admin"]),
		[],
	);

	useEffect(() => {
		if (matchedAppId && matchedAppId !== activeAppId) {
			if (matchedAppId === "sessions" && virtualApps.has(activeAppId)) {
				return;
			}
			setActiveAppId(matchedAppId);
			if (virtualApps.has(matchedAppId) && sessionsRoute) {
				navigate(sessionsRoute, { replace: true });
			}
			return;
		}
		if (!matchedAppId && location.pathname === "/" && apps[0]?.id) {
			setActiveAppId(apps[0].id);
		}
	}, [
		activeAppId,
		apps,
		location.pathname,
		matchedAppId,
		navigate,
		sessionsRoute,
		setActiveAppId,
		virtualApps,
	]);

	useEffect(() => {
		if (activeAppId !== "sessions") return;
		const activeRoute = apps.find((app) => app.id === activeAppId)?.routes?.[0];
		if (!activeRoute || matchedAppId) return;
		const isMatch =
			location.pathname === activeRoute ||
			location.pathname.startsWith(`${activeRoute}/`);
		if (!isMatch) {
			navigate(activeRoute, { replace: true });
		}
	}, [activeAppId, apps, location.pathname, matchedAppId, navigate]);

	// Track shell ready state for coordinated fade-in
	const [shellReady, setShellReady] = useState(false);

	// Avoid hydration mismatch - only render theme-dependent content after mount
	useEffect(() => {
		setMounted(true);
	}, []);

	// Signal shell is ready after mount and trigger preload removal
	useEffect(() => {
		if (!mounted) return;
		// Small delay to ensure layout is stable before revealing
		const timer = requestAnimationFrame(() => {
			requestAnimationFrame(() => {
				setShellReady(true);
				// Remove preload after shell is ready
				document.getElementById("preload")?.remove();
				document.documentElement.removeAttribute("data-preload");
			});
		});
		return () => cancelAnimationFrame(timer);
	}, [mounted]);

	// Use a stable theme value that defaults to "light" during SSR to prevent hydration mismatch
	const currentTheme = mounted ? resolvedTheme : "light";
	const isDark = currentTheme === "dark";

	const ActiveComponent = activeApp?.component ?? null;
	const isSessionsView =
		activeAppId === "sessions" || activeApp?.id === "sessions";

	// Loading bar
	const [barVisible, setBarVisible] = useState(true);
	const [barWidth, setBarWidth] = useState(0);
	const [barFade, setBarFade] = useState(false);

	// Dialog states
	const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
	const [renameDialogOpen, setRenameDialogOpen] = useState(false);
	const [agentPickerOpen, setAgentPickerOpen] = useState(false);
	const [targetSessionId, setTargetSessionId] = useState<string>("");
	const [renameValue, setRenameValue] = useState("");
	const [sessionLimit, setSessionLimit] = useState<number>(3);

	// Command palette
	const { open: commandPaletteOpen, setOpen: setCommandPaletteOpen } =
		useCommandPalette();

	// Expanded state for parent sessions in sidebar
	const [expandedSessions, setExpandedSessions] = useState<Set<string>>(
		new Set(),
	);

	// Expanded state for project groups in sidebar (default: all expanded)
	const [expandedProjects, setExpandedProjects] = useState<Set<string>>(
		() => new Set(["__all__"]), // Start with all expanded
	);

	// Pinned sessions (persisted to localStorage)
	const [pinnedSessions, setPinnedSessions] = useState<Set<string>>(() => {
		if (typeof window === "undefined") return new Set();
		try {
			const stored = localStorage.getItem("octo:pinnedSessions");
			return stored ? new Set(JSON.parse(stored)) : new Set();
		} catch {
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

	// Project dialog states
	const [deleteProjectDialogOpen, setDeleteProjectDialogOpen] = useState(false);
	const [renameProjectDialogOpen, setRenameProjectDialogOpen] = useState(false);
	const [targetProjectKey, setTargetProjectKey] = useState<string>("");
	const [targetProjectName, setTargetProjectName] = useState<string>("");
	const [renameProjectValue, setRenameProjectValue] = useState("");
	const [newProjectDialogOpen, setNewProjectDialogOpen] = useState(false);
	const [projectTemplates, setProjectTemplates] = useState<ProjectTemplateEntry[]>([]);
	const [templatesConfigured, setTemplatesConfigured] = useState(true);
	const [templatesLoading, setTemplatesLoading] = useState(false);
	const [templatesError, setTemplatesError] = useState<string | null>(null);
	const [selectedTemplatePath, setSelectedTemplatePath] = useState<string | null>(
		null,
	);
	const [newProjectPath, setNewProjectPath] = useState("");
	const [newProjectShared, setNewProjectShared] = useState(false);
	const [newProjectSubmitting, setNewProjectSubmitting] = useState(false);
	const [newProjectError, setNewProjectError] = useState<string | null>(null);

	// Project sort state
	const [projectSortBy, setProjectSortBy] = useState<
		"date" | "name" | "sessions"
	>("date");
	const [projectSortAsc, setProjectSortAsc] = useState(false);

	const [selectedProjectKey, setSelectedProjectKey] = useState<string | null>(
		null,
	);
	const [availableAgents, setAvailableAgents] = useState<OpenCodeAgent[]>([]);
	const [workspaceDirectories, setWorkspaceDirectories] = useState<
		{ name: string; path: string; logo?: ProjectLogo }[]
	>([]);

	const runningSessionCount = useMemo(
		() =>
			workspaceSessions.filter((session) => session.status === "running")
				.length,
		[workspaceSessions],
	);
	const sessionLimitLabel = useMemo(() => {
		if (sessionLimit <= 0) return "∞";
		return `${runningSessionCount}/${sessionLimit}`;
	}, [runningSessionCount, sessionLimit]);

	useEffect(() => {
		let mounted = true;
		getSettingsValues("octo")
			.then((values) => {
				if (!mounted) return;
				console.log("[Settings] Loaded values:", values);
				console.log(
					"[Settings] sessions.max_concurrent_sessions:",
					values["sessions.max_concurrent_sessions"],
				);
				const raw = values["sessions.max_concurrent_sessions"]?.value;
				console.log("[Settings] raw value:", raw, "type:", typeof raw);
				if (typeof raw === "number") {
					console.log("[Settings] Setting sessionLimit to:", raw);
					setSessionLimit(raw);
				} else {
					console.log(
						"[Settings] NOT setting sessionLimit, raw is not a number",
					);
				}
			})
			.catch((err) => {
				console.error("Failed to load session limits:", err);
			});
		return () => {
			mounted = false;
		};
	}, []);

	// Pinned projects for filter bar (persisted to localStorage)
	const [pinnedProjects, setPinnedProjects] = useState<string[]>(() => {
		if (typeof window === "undefined") return [];
		try {
			const stored = localStorage.getItem("octo:pinnedProjects");
			return stored ? JSON.parse(stored) : [];
		} catch {
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
	const [directoryPickerOpen, setDirectoryPickerOpen] = useState(false);
	const [directoryPickerPath, setDirectoryPickerPath] = useState(".");
	const [directoryPickerEntries, setDirectoryPickerEntries] = useState<
		{ name: string; path: string }[]
	>([]);
	const [directoryPickerLoading, setDirectoryPickerLoading] = useState(false);
	const [pendingPersona, setPendingPersona] = useState<Persona | null>(null);

	const resetNewProjectForm = useCallback(() => {
		setProjectTemplates([]);
		setTemplatesLoading(false);
		setTemplatesError(null);
		setSelectedTemplatePath(null);
		setNewProjectPath("");
		setNewProjectShared(false);
		setNewProjectSubmitting(false);
		setNewProjectError(null);
	}, []);

	const handleNewProjectDialogChange = useCallback(
		(open: boolean) => {
			setNewProjectDialogOpen(open);
			if (!open) {
				resetNewProjectForm();
			}
		},
		[resetNewProjectForm],
	);

	const refreshWorkspaceDirectories = useCallback(() => {
		if (typeof window === "undefined") return Promise.resolve();
		return listWorkspaceDirectories(".")
			.then((entries) => {
				const dirs = entries.map((entry) => ({
					name: entry.name,
					path: entry.path,
					logo: entry.logo,
				}));
				setWorkspaceDirectories(dirs);
			})
			.catch((err) => {
				console.error("Failed to load workspace directories:", err);
				setWorkspaceDirectories([]);
			});
	}, []);

	const handleProjectDefaultAgentChange = useCallback(
		(projectKey: string, agentId: string) => {
			setProjectDefaultAgents((prev) => {
				if (!agentId) {
					const next = { ...prev };
					delete next[projectKey];
					return next;
				}
				return { ...prev, [projectKey]: agentId };
			});
		},
		[setProjectDefaultAgents],
	);

	const handleNewProjectPathChange = useCallback((value: string) => {
		setNewProjectPath(value);
	}, []);

	const handleCreateProjectFromTemplate = useCallback(async () => {
		setNewProjectError(null);
		if (!selectedTemplatePath) {
			setNewProjectError("Select a template to continue.");
			return;
		}
		const trimmedPath = newProjectPath.trim();
		if (!trimmedPath) {
			setNewProjectError("Project directory is required.");
			return;
		}
		const payload: CreateProjectFromTemplateRequest = {
			template_path: selectedTemplatePath,
			project_path: trimmedPath,
		};
		if (newProjectShared) {
			payload.shared = true;
		}
		setNewProjectSubmitting(true);
		try {
			await createProjectFromTemplate(payload);
			await refreshWorkspaceDirectories();
			handleNewProjectDialogChange(false);
		} catch (err) {
			setNewProjectError(
				err instanceof Error ? err.message : "Failed to create project.",
			);
		} finally {
			setNewProjectSubmitting(false);
		}
	}, [
		selectedTemplatePath,
		newProjectPath,
		newProjectShared,
		refreshWorkspaceDirectories,
		handleNewProjectDialogChange,
	]);

	useEffect(() => {
		if (typeof window === "undefined") return;
		const handleFilter = (event: Event) => {
			const customEvent = event as CustomEvent<string>;
			if (typeof customEvent.detail === "string") {
				setSelectedProjectKey(customEvent.detail);
				setActiveAppId("sessions");
			}
		};
		const handleClear = () => {
			setSelectedProjectKey(null);
		};
		const handleDefaultAgent = (event: Event) => {
			const customEvent = event as CustomEvent<{
				projectKey: string;
				agentId: string;
			}>;
			if (!customEvent.detail) return;
			handleProjectDefaultAgentChange(
				customEvent.detail.projectKey,
				customEvent.detail.agentId,
			);
		};

		window.addEventListener(
			"octo:project-filter",
			handleFilter as EventListener,
		);
		window.addEventListener(
			"octo:project-filter-clear",
			handleClear as EventListener,
		);
		window.addEventListener(
			"octo:project-default-agent",
			handleDefaultAgent as EventListener,
		);
		return () => {
			window.removeEventListener(
				"octo:project-filter",
				handleFilter as EventListener,
			);
			window.removeEventListener(
				"octo:project-filter-clear",
				handleClear as EventListener,
			);
			window.removeEventListener(
				"octo:project-default-agent",
				handleDefaultAgent as EventListener,
			);
		};
	}, [handleProjectDefaultAgentChange, setActiveAppId]);

	useEffect(() => {
		refreshWorkspaceDirectories();
	}, [refreshWorkspaceDirectories]);

	useEffect(() => {
		if (!directoryPickerOpen || typeof window === "undefined") return;
		setDirectoryPickerLoading(true);
		listWorkspaceDirectories(directoryPickerPath)
			.then((entries) => {
				const dirs = entries.map((entry) => ({
					name: entry.name,
					path: entry.path,
				}));
				setDirectoryPickerEntries(dirs);
			})
			.catch((err) => {
				console.error("Failed to load directory picker entries:", err);
				setDirectoryPickerEntries([]);
			})
			.finally(() => setDirectoryPickerLoading(false));
	}, [directoryPickerOpen, directoryPickerPath]);

	useEffect(() => {
		if (!newProjectDialogOpen || typeof window === "undefined") return;
		let active = true;
		setTemplatesLoading(true);
		setTemplatesError(null);
		listProjectTemplates()
			.then((response) => {
				if (!active) return;
				setTemplatesConfigured(response.configured);
				setProjectTemplates(response.templates);
				if (response.templates.length > 0) {
					setSelectedTemplatePath((prev) => prev ?? response.templates[0].path);
				}
			})
			.catch((err) => {
				if (!active) return;
				console.error("Failed to load templates:", err);
				setTemplatesError(
					err instanceof Error ? err.message : "Failed to load templates",
				);
				setProjectTemplates([]);
			})
			.finally(() => {
				if (active) setTemplatesLoading(false);
			});
		return () => {
			active = false;
		};
	}, [newProjectDialogOpen]);

	// Session search
	const [sessionSearch, setSessionSearch] = useState("");
	const deferredSearch = useDeferredValue(sessionSearch);
	// Search mode: "sessions" = filter by name, "messages" = deep search via cass
	const [searchMode, setSearchMode] = useState<SearchMode>("sessions");
	const [agentFilter, setAgentFilter] = useState<AgentFilter>("all");
	const [mainChatTitleHits, setMainChatTitleHits] = useState<CassSearchHit[]>(
		[],
	);

	// Keyboard shortcut: Ctrl+Shift+F to toggle search mode
	useEffect(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			if (e.key === "f" && (e.metaKey || e.ctrlKey) && e.shiftKey) {
				e.preventDefault();
				setSearchMode((prev) =>
					prev === "sessions" ? "messages" : "sessions",
				);
			}
		};
		document.addEventListener("keydown", handleKeyDown);
		return () => document.removeEventListener("keydown", handleKeyDown);
	}, []);

	useEffect(() => {
		const query = deferredSearch.trim().toLowerCase();
		if (searchMode !== "messages" || !query) {
			setMainChatTitleHits([]);
			return;
		}
		let active = true;
		listMainChatPiSessions()
			.then((sessions) => {
				if (!active) return;
				const hits = sessions
					.filter((session) =>
						(session.title ?? "").toLowerCase().includes(query),
					)
					.map((session) => ({
						agent: "pi_agent",
						source_path: `title:pi:${session.id}`,
						session_id: session.id,
						title: session.title ?? "Untitled",
						timestamp: session.modified_at,
						match_type: "title",
						snippet: "Title match",
					}));
				setMainChatTitleHits(hits);
			})
			.catch(() => {
				if (active) setMainChatTitleHits([]);
			});
		return () => {
			active = false;
		};
	}, [deferredSearch, searchMode]);

	const {
		handleMainChatSelect,
		handleMainChatSessionSelect,
		handleMainChatNewSession,
	} = useMainChatNavigation({
		setMainChatAssistantName,
		setMainChatActive,
		setMainChatCurrentSessionId,
		setSelectedChatSessionId,
		setActiveAppId,
		setMobileMenuOpen,
		setMainChatWorkspacePath,
	});

	// Build hierarchical session structure from chatHistory (disk-based, no opencode needed)
	const sessionHierarchy = useMemo(() => {
		// Separate parent and child sessions
		const parentSessions = chatHistory.filter((s) => !s.parent_id);
		const childSessionsByParent = new Map<string, ChatSession[]>();

		for (const session of chatHistory) {
			if (session.parent_id) {
				const children = childSessionsByParent.get(session.parent_id) || [];
				children.push(session);
				childSessionsByParent.set(session.parent_id, children);
			}
		}

		// Sort children by updated time (newest first)
		for (const [parentId, children] of childSessionsByParent) {
			childSessionsByParent.set(
				parentId,
				children.sort((a, b) => b.updated_at - a.updated_at),
			);
		}

		return { parentSessions, childSessionsByParent };
	}, [chatHistory]);

	// Helper to get project key from ChatSession (disk-based) or OpenCodeSession (live)
	const projectKeyForSession = useCallback(
		(
			session:
				| ChatSession
				| { directory?: string | null; projectID?: string | null },
		) => {
			// ChatSession type uses workspace_path/project_name
			if ("workspace_path" in session && session.workspace_path) {
				const normalized = session.workspace_path
					.replace(/\\/g, "/")
					.replace(/\/+$/, "");
				const parts = normalized.split("/").filter(Boolean);
				return parts[parts.length - 1] ?? session.workspace_path;
			}
			// OpenCodeSession type uses directory/projectID
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
			// ChatSession type uses project_name directly
			if ("project_name" in session && session.project_name) {
				return session.project_name;
			}
			// OpenCodeSession type uses directory/projectID
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

	useEffect(() => {
		if (!opencodeBaseUrl) return;
		fetchAgents(opencodeBaseUrl, { directory: opencodeDirectory })
			.then((agents) => {
				setAvailableAgents(agents);
			})
			.catch((err) => {
				console.error("Failed to fetch agents:", err);
				setAvailableAgents([]);
			});
	}, [opencodeBaseUrl, opencodeDirectory]);

	// Filter and sort sessions (pinned first, then by recency)
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
				// Search in title
				if (session.title?.toLowerCase().includes(searchLower)) return true;
				// Search in readable ID (adjective-noun)
				const readableId = generateReadableId(session.id);
				if (readableId.toLowerCase().includes(searchLower)) return true;
				// Search in date (ChatSession uses updated_at)
				if (session.updated_at) {
					const dateStr = formatSessionDate(session.updated_at);
					if (dateStr.toLowerCase().includes(searchLower)) return true;
				}
				// Search in directory name
				if ("workspace_path" in session && session.workspace_path) {
					const dirName = session.workspace_path.split("/").filter(Boolean).pop();
					if (dirName?.toLowerCase().includes(searchLower)) return true;
				}
				return false;
			});
		}

		// Sort: pinned first, then by updated time (ChatSession uses updated_at)
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
		if (searchMode !== "messages" || !query) return [];

		return sessionHierarchy.parentSessions
			.filter((session) => {
				if (!session.title) return false;
				return session.title.toLowerCase().includes(query);
			})
			.map((session) => ({
				agent: "opencode",
				source_path: `title:oc:${session.id}`,
				session_id: session.id,
				title: session.title ?? "Untitled",
				timestamp: session.updated_at,
				match_type: "title",
				snippet: "Title match",
				workspace: session.workspace_path ?? undefined,
			}));
	}, [deferredSearch, searchMode, sessionHierarchy.parentSessions]);

	const messageSearchExtraHits = useMemo(
		() => [...sessionTitleHits, ...mainChatTitleHits],
		[mainChatTitleHits, sessionTitleHits],
	);

	const projectSummaries = useMemo(() => {
		const entries = new Map<
			string,
			{
				key: string;
				name: string;
				directory?: string;
				sessionCount: number;
				lastActive: number;
				logo?: ProjectLogo;
			}
		>();

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
			// ChatSession uses updated_at instead of time.updated
			const lastActive = session.updated_at ?? 0;
			const existing = entries.get(key);
			if (existing) {
				existing.sessionCount += 1;
				if (lastActive > existing.lastActive) existing.lastActive = lastActive;
				// Update directory to absolute path from session if available
				if (session.workspace_path && !existing.directory?.startsWith("/")) {
					existing.directory = session.workspace_path;
				}
			} else {
				entries.set(key, {
					key,
					name,
					// ChatSession uses workspace_path instead of directory
					directory: session.workspace_path ?? undefined,
					sessionCount: 1,
					lastActive,
				});
			}
		}

		if (!entries.has("workspace")) {
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

	// Projects sorted for filter bar: pinned first (in order), then by session count
	const sortedProjectsForFilterBar = useMemo(() => {
		const bySessionCount = [...projectSummaries].sort(
			(a, b) => b.sessionCount - a.sessionCount,
		);
		const pinned: typeof projectSummaries = [];
		const unpinned: typeof projectSummaries = [];

		// First, add pinned projects in their saved order
		for (const key of pinnedProjects) {
			const project = bySessionCount.find((p) => p.key === key);
			if (project) pinned.push(project);
		}

		// Then add unpinned projects sorted by session count
		for (const project of bySessionCount) {
			if (!pinnedProjects.includes(project.key)) {
				unpinned.push(project);
			}
		}

		return [...pinned, ...unpinned];
	}, [projectSummaries, pinnedProjects]);

	const togglePinProject = useCallback((projectKey: string) => {
		setPinnedProjects((prev) => {
			if (prev.includes(projectKey)) {
				return prev.filter((k) => k !== projectKey);
			}
			return [...prev, projectKey];
		});
	}, []);

	const directoryPickerParent = useMemo(() => {
		const normalized = directoryPickerPath.replace(/\\/g, "/");
		if (normalized === "." || normalized === "") return null;
		const parts = normalized.split("/").filter(Boolean);
		if (parts.length <= 1) return ".";
		return parts.slice(0, -1).join("/");
	}, [directoryPickerPath]);

	const recentProjects = useMemo(
		() => projectSummaries.slice(0, 6),
		[projectSummaries],
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

	// Group filtered sessions by project
	const sessionsByProject = useMemo(() => {
		const groups = new Map<
			string,
			{
				key: string;
				name: string;
				directory?: string;
				sessions: typeof filteredSessions;
				logo?: ProjectLogo;
			}
		>();

		for (const session of filteredSessions) {
			const key = projectKeyForSession(session);
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

		// Sort groups: pinned first, then by selected sort criteria
		return [...groups.values()].sort((a, b) => {
			const aPinned = pinnedProjects.includes(a.key);
			const bPinned = pinnedProjects.includes(b.key);
			// Pinned projects come first
			if (aPinned && !bPinned) return -1;
			if (!aPinned && bPinned) return 1;

			// Within same pin status, sort by selected criteria
			let comparison = 0;
			if (projectSortBy === "date") {
				const aLatest = Math.max(...a.sessions.map((s) => s.updated_at ?? 0));
				const bLatest = Math.max(...b.sessions.map((s) => s.updated_at ?? 0));
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
		projectKeyForSession,
		projectLabelForSession,
		projectSummaries,
		pinnedProjects,
		projectSortBy,
		projectSortAsc,
	]);

	const handleProjectSelect = useCallback(
		(projectKey: string) => {
			setSelectedProjectKey(projectKey);
			setActiveAppId("sessions");
			if (sessionsRoute) {
				navigate(sessionsRoute);
			}
			setMobileMenuOpen(false);
		},
		[navigate, sessionsRoute, setActiveAppId],
	);

	const handleProjectClear = useCallback(() => {
		setSelectedProjectKey(null);
	}, []);

	const resolveWorkspacePath = useCallback(
		(path: string) => {
			const basePath = selectedWorkspaceSession?.workspace_path;
			if (!basePath) return path;
			if (path.startsWith("/")) return path;
			if (path === "." || path.trim() === "") return basePath;
			const joined = `${basePath}/${path}`;
			const normalized = joined.split("/").filter(Boolean).join("/");
			return basePath.startsWith("/") ? `/${normalized}` : normalized;
		},
		[selectedWorkspaceSession?.workspace_path],
	);

	const handleDirectoryConfirm = useCallback(
		async (path: string) => {
			if (!pendingPersona) return;
			// The path from directory picker is already relative to workspace root
			// Don't resolve it relative to current session - pass it directly
			// The backend will expand it relative to the workspace root
			setDirectoryPickerOpen(false);
			setPendingPersona(null);
			setActiveAppId("sessions");
			if (sessionsRoute) {
				navigate(sessionsRoute);
			}
			await createNewChatWithPersona(pendingPersona, path);
		},
		[createNewChatWithPersona, navigate, pendingPersona, sessionsRoute, setActiveAppId],
	);

	const handleDirectoryPickerOpenChange = useCallback((open: boolean) => {
		setDirectoryPickerOpen(open);
		if (!open) {
			setPendingPersona(null);
		}
	}, []);

	// Handle session click - select session and switch to chats view
	const handleSessionClick = (sessionId: string) => {
		setSelectedChatSessionId(sessionId);
		setActiveAppId("sessions");
		if (sessionsRoute) {
			navigate(sessionsRoute);
		}
		setMobileMenuOpen(false);
		// Clear main chat selection when clicking a regular session
		setMainChatActive(false);
		setMainChatWorkspacePath(null);
	};

	// Handle search result click - navigate to the session and scroll to message
	const handleSearchResultClick = useCallback(
		(hit: CassSearchHit) => {
			// Clear search and switch back to sessions mode
			setSessionSearch("");
			setSearchMode("sessions");

			// Set scroll target - use message_id if available, otherwise construct from line_number
			const targetMessageId =
				hit.message_id || (hit.line_number ? `line-${hit.line_number}` : null);
			if (targetMessageId) {
				setScrollToMessageId(targetMessageId);
			}

			if (hit.agent === "pi_agent") {
				// Navigate to Main Chat
				setActiveAppId("sessions");
				if (sessionsRoute) {
					navigate(sessionsRoute);
				}
				setMainChatActive(true);
				// Extract workspace from hit if available
				if (hit.workspace) {
					setMainChatWorkspacePath(hit.workspace);
				}
			} else if (hit.agent === "opencode" || hit.agent === "claude_code") {
				// Navigate to OpenCode session
				// Extract session ID from source_path if not provided
				const sessionId =
					hit.session_id ||
					hit.source_path.match(/ses_[a-zA-Z0-9]+/)?.[0] ||
					"";
				if (sessionId) {
					setSelectedChatSessionId(sessionId);
					setActiveAppId("sessions");
					if (sessionsRoute) {
						navigate(sessionsRoute);
					}
					setMainChatActive(false);
					setMainChatWorkspacePath(null);
				}
			}
			setMobileMenuOpen(false);
		},
		[
			setActiveAppId,
			setMainChatActive,
			setMainChatWorkspacePath,
			setSelectedChatSessionId,
			setScrollToMessageId,
			navigate,
			sessionsRoute,
		],
	);

	// Context menu handlers
	const handlePinSession = useCallback((sessionId: string) => {
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

	const handleRenameSession = useCallback(
		(sessionId: string) => {
			// Use chatHistory (disk-based) to find session title
			const session = chatHistory.find((s) => s.id === sessionId);
			setTargetSessionId(sessionId);
			setRenameValue(session?.title || "");
			setRenameDialogOpen(true);
		},
		[chatHistory],
	);

	const handleConfirmRename = useCallback(async () => {
		if (targetSessionId && renameValue.trim()) {
			await renameChatSession(targetSessionId, renameValue.trim());
		}
		setRenameDialogOpen(false);
		setTargetSessionId("");
		setRenameValue("");
	}, [targetSessionId, renameValue, renameChatSession]);

	const handleDeleteSession = useCallback((sessionId: string) => {
		setTargetSessionId(sessionId);
		setDeleteDialogOpen(true);
	}, []);

	const handleConfirmDelete = useCallback(async () => {
		if (targetSessionId) {
			// Find the session's workspace path from chat history
			const session = chatHistory.find((s) => s.id === targetSessionId);
			const workspacePath = session?.workspace_path;

			// Determine the baseUrl to use for deletion
			let baseUrl: string | null = opencodeBaseUrl;

			// If we have a workspace path and no opencode running, start it first
			if (workspacePath && workspacePath !== "global" && !baseUrl) {
				baseUrl = await ensureOpencodeRunning(workspacePath);
			}

			if (baseUrl) {
				await deleteChatSession(targetSessionId, baseUrl);
			}
		}
		setDeleteDialogOpen(false);
		setTargetSessionId("");
	}, [
		targetSessionId,
		deleteChatSession,
		chatHistory,
		opencodeBaseUrl,
		ensureOpencodeRunning,
	]);

	// Project handlers
	const handlePinProject = useCallback((projectKey: string) => {
		setPinnedProjects((prev) => {
			if (prev.includes(projectKey)) {
				return prev.filter((k) => k !== projectKey);
			}
			return [...prev, projectKey];
		});
	}, []);

	const handleRenameProject = useCallback(
		(projectKey: string, currentName: string) => {
			setTargetProjectKey(projectKey);
			setTargetProjectName(currentName);
			setRenameProjectValue(currentName);
			setRenameProjectDialogOpen(true);
		},
		[],
	);

	const handleConfirmRenameProject = useCallback(async () => {
		if (targetProjectKey && renameProjectValue.trim()) {
			// Find all sessions belonging to this project and rename them
			// For now, we'll just close the dialog - actual renaming would need backend support
			// TODO: Implement project rename via backend API
			console.log(
				"[handleConfirmRenameProject] Would rename project:",
				targetProjectKey,
				"to:",
				renameProjectValue.trim(),
			);
		}
		setRenameProjectDialogOpen(false);
		setTargetProjectKey("");
		setTargetProjectName("");
		setRenameProjectValue("");
	}, [targetProjectKey, renameProjectValue]);

	const handleDeleteProject = useCallback(
		(projectKey: string, projectName: string) => {
			setTargetProjectKey(projectKey);
			setTargetProjectName(projectName);
			setDeleteProjectDialogOpen(true);
		},
		[],
	);

	const handleConfirmDeleteProject = useCallback(async () => {
		if (targetProjectKey) {
			// Find all sessions belonging to this project
			const sessionsToDelete = chatHistory.filter((s) => {
				const key = s.workspace_path
					? s.workspace_path.split("/").filter(Boolean).pop() || "global"
					: "global";
				return key === targetProjectKey;
			});

			// Delete each session
			for (const session of sessionsToDelete) {
				const workspacePath = session.workspace_path;
				let baseUrl: string | null = opencodeBaseUrl;

				if (workspacePath && workspacePath !== "global" && !baseUrl) {
					baseUrl = await ensureOpencodeRunning(workspacePath);
				}

				if (baseUrl) {
					await deleteChatSession(session.id, baseUrl);
				}
			}
		}
		setDeleteProjectDialogOpen(false);
		setTargetProjectKey("");
		setTargetProjectName("");
	}, [
		targetProjectKey,
		chatHistory,
		deleteChatSession,
		opencodeBaseUrl,
		ensureOpencodeRunning,
	]);

	const handleNewChat = useCallback(async () => {
		console.log("[handleNewChat] called", {
			mainChatActive,
			selectedWorkspaceSession: !!selectedWorkspaceSession,
			opencodeBaseUrl,
			selectedProjectKey,
			projectSummaries: projectSummaries.map((p) => ({
				key: p.key,
				directory: p.directory,
			})),
		});

		// If Main Chat is active, create a new Main Chat session
		if (mainChatActive) {
			console.log("[handleNewChat] Creating new Main Chat session");
			setActiveAppId("sessions");
			requestNewMainChatSession();
			return;
		}

		// Check if we have a project filter selected - prioritize this over active session
		if (selectedProjectKey) {
			const project = projectSummaries.find(
				(p) => p.key === selectedProjectKey,
			);
			console.log("[handleNewChat] Project filter selected:", {
				selectedProjectKey,
				project,
			});
			if (project?.directory) {
				console.log(
					"[handleNewChat] Starting session for project:",
					project.directory,
				);
				setActiveAppId("sessions");
				const optimisticId = createOptimisticChatSession(project.directory);
				const baseUrl = await ensureOpencodeRunning(project.directory);
				console.log("[handleNewChat] Got baseUrl:", baseUrl);
				if (baseUrl) {
					await createNewChat(baseUrl, project.directory, { optimisticId });
					return;
				}
				clearOptimisticChatSession(optimisticId);
			}
		}

		// If we have a running workspace session, create a new chat in it
		if (selectedWorkspaceSession && opencodeBaseUrl) {
			console.log("[handleNewChat] Using existing workspace session");
			setActiveAppId("sessions");
			await createNewChat();
			return;
		}

		// Check if we have a workspace path from the current chat history
		// This happens when viewing a historical chat without a running session
		const currentWorkspacePath = selectedChatFromHistory?.workspace_path;
		if (currentWorkspacePath && currentWorkspacePath !== "global") {
			// Start a session for this workspace and create a new chat
			console.log(
				"[handleNewChat] Using workspace from history:",
				currentWorkspacePath,
			);
			setActiveAppId("sessions");
			const optimisticId = createOptimisticChatSession(currentWorkspacePath);
			const baseUrl = await ensureOpencodeRunning(currentWorkspacePath);
			if (baseUrl) {
				await createNewChat(baseUrl, currentWorkspacePath, {
					optimisticId,
				});
				return;
			}
			clearOptimisticChatSession(optimisticId);
		}

		// No workspace context - open persona picker to select one
		console.log("[handleNewChat] Opening agent picker");
		setAgentPickerOpen(true);
	}, [
		mainChatActive,
		requestNewMainChatSession,
		selectedWorkspaceSession,
		opencodeBaseUrl,
		selectedChatFromHistory,
		selectedProjectKey,
		projectSummaries,
		ensureOpencodeRunning,
		createNewChat,
		createOptimisticChatSession,
		clearOptimisticChatSession,
		setActiveAppId,
	]);

	// Create a new chat in a specific project directory
	const handleNewChatInProject = useCallback(
		async (directory: string) => {
			setActiveAppId("sessions");
			setMobileMenuOpen(false);
			const optimisticId = createOptimisticChatSession(directory);
			const baseUrl = await ensureOpencodeRunning(directory);
			if (baseUrl) {
				await createNewChat(baseUrl, directory, { optimisticId });
				return;
			}
			clearOptimisticChatSession(optimisticId);
		},
		[
			ensureOpencodeRunning,
			createNewChat,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			setActiveAppId,
		],
	);

	const handleAgentSelect = useCallback(
		async (persona: Persona) => {
			if (persona.workspace_mode === "ask") {
				setPendingPersona(persona);
				setDirectoryPickerPath(".");
				setDirectoryPickerOpen(true);
				return;
			}
			setActiveAppId("sessions");
			await createNewChatWithPersona(persona);
		},
		[createNewChatWithPersona, setActiveAppId],
	);

	useEffect(() => {
		if (typeof window === "undefined") return;

		// iOS PWA: keep viewport height stable after app resume
		const applyViewportHeight = () => {
			const height = window.visualViewport?.height ?? window.innerHeight;
			document.documentElement.style.setProperty(
				"--app-viewport-height",
				`${height}px`,
			);
		};

		applyViewportHeight();
		window.visualViewport?.addEventListener("resize", applyViewportHeight);
		window.visualViewport?.addEventListener("scroll", applyViewportHeight);
		window.addEventListener("orientationchange", applyViewportHeight);
		window.addEventListener("pageshow", applyViewportHeight);
		document.addEventListener("visibilitychange", applyViewportHeight);

		// Top loading bar animation
		setBarVisible(true);
		setBarWidth(25);
		const growTimer = window.setTimeout(() => setBarWidth(80), 150);
		const finish = () => {
			setBarWidth(100);
			setBarFade(true);
			window.setTimeout(() => setBarVisible(false), 500);
		};
		window.addEventListener("load", finish, { once: true });
		const fallback = window.setTimeout(finish, 1600);
		return () => {
			window.visualViewport?.removeEventListener("resize", applyViewportHeight);
			window.visualViewport?.removeEventListener("scroll", applyViewportHeight);
			window.removeEventListener("orientationchange", applyViewportHeight);
			window.removeEventListener("pageshow", applyViewportHeight);
			document.removeEventListener("visibilitychange", applyViewportHeight);
			window.clearTimeout(growTimer);
			window.clearTimeout(fallback);
			window.removeEventListener("load", finish);
		};
	}, []);

	const toggleTheme = () => {
		const next = isDark ? "light" : "dark";
		// Disable transitions during theme switch
		document.documentElement.classList.add("no-transitions");
		setTheme(next);
		// Re-enable transitions after a brief delay
		requestAnimationFrame(() => {
			requestAnimationFrame(() => {
				document.documentElement.classList.remove("no-transitions");
			});
		});
	};

	const toggleLocale = () => {
		const next = locale === "de" ? "en" : "de";
		setLocale(next);
	};

	const shellBg = "var(--background)";
	const sidebarBg = "var(--sidebar, #181b1a)";
	const navIdle = "var(--sidebar, #181b1a)";
	const sidebarHover = "rgba(59, 167, 124, 0.12)";
	const sidebarHoverBorder = "transparent";
	const navText = "var(--sidebar-foreground, #dfe5e1)";
	const navActiveBg = "#3ba77c";
	const navActiveText = "#0b0f0d";
	const navActiveBorder = "#3ba77c";

	const navIconFor = useCallback((id: string) => {
		switch (id) {
			case "projects":
				return FolderKanban;
			case "sessions":
				return MessageSquare;
			case "agents":
				return Bot;
			case "admin":
				return Shield;
			default:
				return FolderKanban;
		}
	}, []);

	const activateApp = useCallback(
		(appId: string) => {
			setActiveAppId(appId);
			const route = apps.find((app) => app.id === appId)?.routes?.[0];
			if (!route) return;
			if (virtualApps.has(appId)) {
				if (sessionsRoute) {
					navigate(sessionsRoute);
				}
				return;
			}
			navigate(route);
		},
		[apps, navigate, sessionsRoute, setActiveAppId, virtualApps],
	);

	// Toggle app - if already active, go back to sessions
	const toggleApp = useCallback(
		(appId: string) => {
			if (activeAppId === appId) {
				// Already active, go back to sessions
				setActiveAppId("sessions");
				const sessionsRoute = apps.find((app) => app.id === "sessions")
					?.routes?.[0];
				if (sessionsRoute) {
					navigate(sessionsRoute);
				}
			} else {
				activateApp(appId);
			}
		},
		[activeAppId, activateApp, apps, navigate, setActiveAppId],
	);

	const handleMobileNavClick = (appId: string) => {
		activateApp(appId);
		setMobileMenuOpen(false);
	};

	// Toggle version for mobile (for settings/admin)
	const handleMobileToggleClick = (appId: string) => {
		if (activeAppId === appId) {
			// Already active, go back to sessions
			activateApp("sessions");
		} else {
			activateApp(appId);
		}
		setMobileMenuOpen(false);
	};

	return (
		<div
			className="flex min-h-screen bg-background text-foreground overflow-hidden transition-opacity duration-300 ease-out"
			style={{
				opacity: shellReady ? 1 : 0,
				height: "var(--app-viewport-height, 100vh)",
			}}
		>
			{/* Mobile header */}
			<header
				className="fixed top-0 left-0 right-0 flex items-center px-3 z-50 md:hidden h-[calc(3.5rem+env(safe-area-inset-top))]"
				style={{
					backgroundColor: sidebarBg,
					paddingTop: "env(safe-area-inset-top)",
				}}
			>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					aria-label="Menu"
					onClick={() => setMobileMenuOpen(true)}
					className="text-muted-foreground hover:text-primary flex-shrink-0"
				>
					<Menu className="w-5 h-5" />
				</Button>
				{/* Header title */}
				{activeAppId === "sessions" ? (
					selectedChatFromHistory ? (
						<div className="flex-1 min-w-0 px-3 text-center">
							<div className="text-sm font-medium text-foreground truncate">
								{selectedChatFromHistory.title
									?.replace(
										/\s*-\s*\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z?$/,
										"",
									)
									.trim() || "Chat"}
							</div>
							<div className="text-[10px] text-muted-foreground truncate">
								{opencodeDirectory && (
									<span className="font-medium">
										{opencodeDirectory.split("/").filter(Boolean).pop()}
										{" | "}
									</span>
								)}
								{generateReadableId(selectedChatFromHistory.id)}
								{selectedChatFromHistory.updated_at && (
									<span className="opacity-60">
										{" "}
										| {formatSessionDate(selectedChatFromHistory.updated_at)}
									</span>
								)}
							</div>
						</div>
					) : mainChatActive ? (
						<div className="flex-1 min-w-0 px-3 text-center">
							<div className="text-sm font-medium text-foreground truncate">
								{mainChatAssistantName ||
									(locale === "de" ? "Hauptchat" : "Main Chat")}
							</div>
							<div className="text-[10px] text-muted-foreground truncate">
								{locale === "de" ? "Hauptchat" : "Main Chat"}
							</div>
						</div>
					) : (
						<div className="flex-1 flex justify-center">
							<img
								src={
									isDark ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"
								}
								alt="OCTO"
								width={80}
								height={32}
								className="h-8 w-auto object-contain"
							/>
						</div>
					)
				) : (
					<div className="flex-1 min-w-0 px-3 text-center">
						<div className="text-sm font-medium text-foreground truncate">
							{activeApp?.label ? resolveText(activeApp.label) : "Octo"}
						</div>
						<div className="text-[10px] text-muted-foreground truncate">
							{activeApp?.description || ""}
						</div>
					</div>
				)}
				{/* New chat button */}
				{activeAppId === "sessions" && (
					<Button
						type="button"
						variant="ghost"
						size="icon"
						aria-label={locale === "de" ? "Neuer Chat" : "New Chat"}
						onClick={handleNewChat}
						className="text-muted-foreground hover:text-primary flex-shrink-0"
					>
						<Plus className="w-5 h-5" />
					</Button>
				)}
			</header>

			{/* Mobile fullscreen menu */}
			{mobileMenuOpen && (
				<div
					className="fixed inset-0 z-50 flex flex-col md:hidden"
					style={{
						backgroundColor: sidebarBg,
						paddingTop: "env(safe-area-inset-top)",
					}}
				>
					<div className="h-14 flex items-center justify-between px-3">
						<img
							src={
								isDark ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"
							}
							alt="OCTO"
							width={70}
							height={28}
							className="h-7 w-auto object-contain flex-shrink-0"
						/>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							aria-label="Close menu"
							onClick={() => setMobileMenuOpen(false)}
							className="text-muted-foreground hover:text-primary flex-shrink-0"
						>
							<X className="w-5 h-5" />
						</Button>
					</div>

					<div className="w-full px-4">
						<div className="h-px w-full bg-primary/50" />
					</div>

					<nav className="flex-1 w-full px-3 pt-3 flex flex-col min-h-0 overflow-x-hidden">
						{chatHistory.length > 0 && (
							<div className="flex-1 min-h-0 flex flex-col">
								{/* Sticky header section - Search, Main Chat, Sessions header */}
								<div className="flex-shrink-0 space-y-0.5 px-1">
									{/* Mobile search input */}
									<div className="relative px-1 mb-2">
										<DropdownMenu>
											<DropdownMenuTrigger asChild>
												<button
													type="button"
													className={cn(
														"absolute left-2 top-1/2 -translate-y-1/2 flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] font-medium transition-colors",
														searchMode === "messages"
															? "bg-primary/20 text-primary"
															: "text-muted-foreground hover:text-foreground hover:bg-sidebar-accent",
													)}
													title="Ctrl+Shift+F"
												>
													{searchMode === "messages" ? (
														<MessageSquare className="w-3 h-3" />
													) : (
														<Search className="w-3 h-3" />
													)}
													<ChevronDown className="w-2.5 h-2.5" />
												</button>
											</DropdownMenuTrigger>
											<DropdownMenuContent align="start" className="w-48">
												<DropdownMenuItem
													onClick={() => setSearchMode("sessions")}
													className={cn(
														searchMode === "sessions" && "bg-accent",
													)}
												>
													<Search className="w-3.5 h-3.5 mr-2" />
													{locale === "de"
														? "Sitzungen filtern"
														: "Filter sessions"}
												</DropdownMenuItem>
												<DropdownMenuItem
													onClick={() => setSearchMode("messages")}
													className={cn(
														searchMode === "messages" && "bg-accent",
													)}
												>
													<MessageSquare className="w-3.5 h-3.5 mr-2" />
													{locale === "de"
														? "Nachrichten suchen"
														: "Search messages"}
												</DropdownMenuItem>
												{searchMode === "messages" && (
													<>
														<DropdownMenuSeparator />
														<DropdownMenuItem
															onClick={() => setAgentFilter("all")}
															className={cn(
																agentFilter === "all" && "bg-accent",
															)}
														>
															{locale === "de"
																? "Alle Agenten"
																: "All agents"}
														</DropdownMenuItem>
														<DropdownMenuItem
															onClick={() => setAgentFilter("opencode")}
															className={cn(
																agentFilter === "opencode" && "bg-accent",
															)}
														>
															{locale === "de"
																? "Nur OpenCode"
																: "OpenCode only"}
														</DropdownMenuItem>
														<DropdownMenuItem
															onClick={() => setAgentFilter("pi_agent")}
															className={cn(
																agentFilter === "pi_agent" && "bg-accent",
															)}
														>
															{locale === "de"
																? "Nur Main Chat"
																: "Main Chat only"}
														</DropdownMenuItem>
													</>
												)}
											</DropdownMenuContent>
										</DropdownMenu>
										<input
											type="text"
											placeholder={
												searchMode === "messages"
													? locale === "de"
														? "Nachrichten durchsuchen..."
														: "Search messages..."
													: locale === "de"
														? "Suchen..."
														: "Search..."
											}
											value={sessionSearch}
											onChange={(e) => setSessionSearch(e.target.value)}
											className="w-full pl-12 pr-10 py-2 text-sm bg-sidebar-accent/50 border border-sidebar-border rounded placeholder:text-muted-foreground/50 focus:outline-none focus:border-primary/50"
										/>
										{sessionSearch && (
											<button
												type="button"
												onClick={() => {
													setSessionSearch("");
													setSearchMode("sessions");
												}}
												className="absolute right-3 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground"
											>
												<X className="w-4 h-4" />
											</button>
										)}
									</div>
									{/* Main Chat */}
									<div className="mb-2 pb-2">
										<MainChatEntry
											isSelected={mainChatActive}
											activeSessionId={
												mainChatActive ? mainChatCurrentSessionId : null
											}
											onSelect={handleMainChatSelect}
											onSessionSelect={handleMainChatSessionSelect}
											onNewSession={handleMainChatNewSession}
											locale={locale}
										/>
									</div>
									{/* Sessions header - between search and chat list */}
									<div className="flex items-center justify-between gap-2 px-2 py-1.5">
										<div className="flex items-center gap-2">
											<span className="text-xs uppercase tracking-wide text-muted-foreground">
												{locale === "de" ? "Sitzungen" : "Sessions"}
											</span>
											<span className="text-xs text-muted-foreground/50">
												({filteredSessions.length}
												{deferredSearch ? `/${chatHistory.length}` : ""})
											</span>
										</div>
								<div className="flex items-center gap-1">
									<button
										type="button"
										onClick={handleNewChat}
										className="p-1 text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded"
										title={locale === "de" ? "Neue Sitzung" : "New session"}
									>
										<Plus className="w-3 h-3" />
									</button>
									<button
										type="button"
										onClick={() => setNewProjectDialogOpen(true)}
										className="p-1 text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded"
										title={locale === "de" ? "Neues Projekt" : "New project"}
									>
										<FolderPlus className="w-3 h-3" />
									</button>
									{selectedProjectLabel && (
										<button
											type="button"
													onClick={handleProjectClear}
													className="flex items-center gap-1 text-[10px] text-muted-foreground/70 hover:text-foreground"
												>
													<X className="w-3 h-3" />
													{selectedProjectLabel}
												</button>
											)}
											{/* Sort dropdown */}
											<DropdownMenu>
												<DropdownMenuTrigger asChild>
													<button
														type="button"
														className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded"
														title={locale === "de" ? "Sortieren" : "Sort"}
													>
														{projectSortAsc ? (
															<ArrowUp className="w-4 h-4" />
														) : (
															<ArrowDown className="w-4 h-4" />
														)}
													</button>
												</DropdownMenuTrigger>
												<DropdownMenuContent align="end" className="w-40">
													<DropdownMenuItem
														onClick={() => setProjectSortBy("date")}
														className={cn(
															projectSortBy === "date" && "bg-accent",
														)}
													>
														<Clock className="w-4 h-4 mr-2" />
														{locale === "de" ? "Datum" : "Date"}
													</DropdownMenuItem>
													<DropdownMenuItem
														onClick={() => setProjectSortBy("name")}
														className={cn(
															projectSortBy === "name" && "bg-accent",
														)}
													>
														<ArrowUpDown className="w-4 h-4 mr-2" />
														{locale === "de" ? "Name" : "Name"}
													</DropdownMenuItem>
													<DropdownMenuItem
														onClick={() => setProjectSortBy("sessions")}
														className={cn(
															projectSortBy === "sessions" && "bg-accent",
														)}
													>
														<MessageSquare className="w-4 h-4 mr-2" />
														{locale === "de" ? "Anzahl" : "Count"}
													</DropdownMenuItem>
													<DropdownMenuSeparator />
													<DropdownMenuItem
														onClick={() => setProjectSortAsc(!projectSortAsc)}
													>
														{projectSortAsc ? (
															<>
																<ArrowDown className="w-4 h-4 mr-2" />
																{locale === "de" ? "Absteigend" : "Descending"}
															</>
														) : (
															<>
																<ArrowUp className="w-4 h-4 mr-2" />
																{locale === "de" ? "Aufsteigend" : "Ascending"}
															</>
														)}
													</DropdownMenuItem>
												</DropdownMenuContent>
											</DropdownMenu>
										</div>
									</div>
								</div>
								{/* Scrollable chat list - grouped by project */}
								<div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden space-y-1 px-1">
									{searchMode === "messages" && sessionSearch.trim() ? (
										<SearchResults
											query={sessionSearch}
											agentFilter={agentFilter}
											locale={locale}
											onResultClick={handleSearchResultClick}
											extraHits={messageSearchExtraHits}
											className="mb-2"
										/>
									) : (
										<>
											{filteredSessions.length === 0 && deferredSearch && (
												<div className="text-sm text-muted-foreground/50 text-center py-4">
													{locale === "de" ? "Keine Ergebnisse" : "No results"}
												</div>
											)}
											{sessionsByProject.map((project) => {
												// Auto-expand all when searching
												const isProjectExpanded =
													deferredSearch || expandedProjects.has(project.key);
												const isProjectPinned = pinnedProjects.includes(
													project.key,
												);
												return (
													<div
														key={project.key}
														className="border-b border-sidebar-border/50 last:border-b-0"
													>
												{/* Project header */}
												<ContextMenu>
													<ContextMenuTrigger className="contents">
														<div className="flex items-center gap-1 px-1 py-1.5 group">
															<button
																type="button"
																onClick={() =>
																	toggleProjectExpanded(project.key)
																}
																className="flex-1 flex items-center gap-1.5 text-left hover:bg-sidebar-accent/50 px-1 py-0.5 -mx-1"
															>
																{isProjectExpanded ? (
																	<ChevronDown className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
																) : (
																	<ChevronRight className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
																)}
																{isProjectPinned && (
																	<Pin className="w-3.5 h-3.5 text-primary/70 flex-shrink-0" />
																)}
																<FolderKanban className="w-4 h-4 text-primary/70 flex-shrink-0" />
																<span className="text-sm font-medium text-foreground truncate">
																	{project.name}
																</span>
																<span className="text-xs text-muted-foreground">
																	({project.sessions.length})
																</span>
															</button>
															{project.directory ? (
																<button
																	type="button"
																	onClick={() =>
																		handleNewChatInProject(project.directory)
																	}
																	className="p-1.5 text-muted-foreground hover:text-primary hover:bg-sidebar-accent opacity-0 group-hover:opacity-100 transition-opacity"
																	title={
																		locale === "de"
																			? "Neuer Chat in diesem Projekt"
																			: "New chat in this project"
																	}
																>
																	<Plus className="w-4 h-4" />
																</button>
															) : null}
														</div>
													</ContextMenuTrigger>
													<ContextMenuContent>
														{project.directory && (
															<>
																<ContextMenuItem
																	onClick={() =>
																		handleNewChatInProject(project.directory)
																	}
																>
																	<Plus className="w-4 h-4 mr-2" />
																	{locale === "de"
																		? "Neue Sitzung"
																		: "New Session"}
																</ContextMenuItem>
																<ContextMenuSeparator />
															</>
														)}
														<ContextMenuItem
															onClick={() => handlePinProject(project.key)}
														>
															<Pin className="w-4 h-4 mr-2" />
															{isProjectPinned
																? locale === "de"
																	? "Lospinnen"
																	: "Unpin"
																: locale === "de"
																	? "Anpinnen"
																	: "Pin"}
														</ContextMenuItem>
														<ContextMenuItem
															onClick={() =>
																handleRenameProject(project.key, project.name)
															}
														>
															<Pencil className="w-4 h-4 mr-2" />
															{locale === "de" ? "Umbenennen" : "Rename"}
														</ContextMenuItem>
														<ContextMenuSeparator />
														<ContextMenuItem
															variant="destructive"
															onClick={() =>
																handleDeleteProject(project.key, project.name)
															}
														>
															<Trash2 className="w-4 h-4 mr-2" />
															{locale === "de" ? "Loschen" : "Delete"} (
															{project.sessions.length}{" "}
															{project.sessions.length === 1 ? "chat" : "chats"}
															)
														</ContextMenuItem>
													</ContextMenuContent>
												</ContextMenu>
												{/* Project sessions */}
												{isProjectExpanded && (
													<div className="space-y-0.5 pb-1">
														{project.sessions.map((session) => {
															const isSelected =
																selectedChatSessionId === session.id;
															const children =
																sessionHierarchy.childSessionsByParent.get(
																	session.id,
																) || [];
															const hasChildren = children.length > 0;
															const isExpanded = expandedSessions.has(
																session.id,
															);
															const readableId = generateReadableId(session.id);
															const formattedDate = session.updated_at
																? formatSessionDate(session.updated_at)
																: null;
															return (
																<div key={session.id} className="ml-4">
																	<ContextMenu>
																		<ContextMenuTrigger className="contents">
																			<div
																				className={cn(
																					"w-full px-2 py-2 text-left transition-colors flex items-start gap-1.5 cursor-pointer",
																					isSelected
																						? "bg-primary/15 border border-primary text-foreground"
																						: "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
																				)}
																			>
																				{hasChildren ? (
																					<button
																						type="button"
																						onClick={() =>
																							toggleSessionExpanded(session.id)
																						}
																						className="mt-0.5 p-1 hover:bg-muted rounded flex-shrink-0 cursor-pointer"
																					>
																						{isExpanded ? (
																							<ChevronDown className="w-4 h-4" />
																						) : (
																							<ChevronRight className="w-4 h-4" />
																						)}
																					</button>
																				) : (
																					<MessageSquare className="w-4 h-4 mt-0.5 flex-shrink-0 text-primary/70" />
																				)}
																				<button
																					type="button"
																					onClick={() =>
																						handleSessionClick(session.id)
																					}
																					className="flex-1 min-w-0 text-left"
																				>
																					<div className="flex items-center gap-1">
																						{pinnedSessions.has(session.id) && (
																							<Pin className="w-3 h-3 flex-shrink-0 text-primary/70" />
																						)}
																						<span className="text-sm truncate font-medium">
																							{session.title || "Untitled"}
																						</span>
																						{hasChildren && (
																							<span className="text-xs text-primary/70">
																								({children.length})
																							</span>
																						)}
																						{busySessions.has(session.id) && (
																							<Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
																						)}
																					</div>
																					{formattedDate && (
																						<div className="text-[11px] text-muted-foreground/50 mt-0.5">
																							{formattedDate}
																						</div>
																					)}
																				</button>
																			</div>
																		</ContextMenuTrigger>
																		<ContextMenuContent>
																			<ContextMenuItem
																				onClick={() => {
																					navigator.clipboard.writeText(
																						readableId,
																					);
																				}}
																			>
																				<Copy className="w-4 h-4 mr-2" />
																				{readableId}
																			</ContextMenuItem>
																			<ContextMenuSeparator />
																			<ContextMenuItem
																				onClick={() =>
																					handlePinSession(session.id)
																				}
																			>
																				<Pin className="w-4 h-4 mr-2" />
																				{pinnedSessions.has(session.id)
																					? locale === "de"
																						? "Lospinnen"
																						: "Unpin"
																					: locale === "de"
																						? "Anpinnen"
																						: "Pin"}
																			</ContextMenuItem>
																			<ContextMenuItem
																				onClick={() =>
																					handleRenameSession(session.id)
																				}
																			>
																				<Pencil className="w-4 h-4 mr-2" />
																				{locale === "de"
																					? "Umbenennen"
																					: "Rename"}
																			</ContextMenuItem>
																			<ContextMenuSeparator />
																			<ContextMenuItem
																				variant="destructive"
																				onClick={() =>
																					handleDeleteSession(session.id)
																				}
																			>
																				<Trash2 className="w-4 h-4 mr-2" />
																				{locale === "de" ? "Loschen" : "Delete"}
																			</ContextMenuItem>
																		</ContextMenuContent>
																	</ContextMenu>
																	{/* Child sessions (subagents) in mobile */}
																	{hasChildren && isExpanded && (
																		<div className="ml-6 border-l border-muted pl-2 space-y-1 mt-1">
																			{children.map((child) => {
																				const isChildSelected =
																					selectedChatSessionId === child.id;
																				const childFormattedDate =
																					child.updated_at
																						? formatSessionDate(
																								child.updated_at,
																							)
																						: null;
																				return (
																					<button
																						type="button"
																						key={child.id}
																						onClick={() =>
																							handleSessionClick(child.id)
																						}
																						className={cn(
																							"w-full px-2 py-2 text-left transition-colors text-sm",
																							isChildSelected
																								? "bg-primary/15 border border-primary text-foreground"
																								: "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
																						)}
																					>
																						<div className="flex items-center gap-1">
																							<Bot className="w-3.5 h-3.5 flex-shrink-0 text-primary/70" />
																							<span className="truncate font-medium">
																								{child.title || "Subagent"}
																							</span>
																							{busySessions.has(child.id) && (
																								<Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
																							)}
																						</div>
																						{childFormattedDate && (
																							<div className="text-xs text-muted-foreground/50 mt-0.5 ml-5">
																								{childFormattedDate}
																							</div>
																						)}
																					</button>
																				);
																			})}
																		</div>
																	)}
																</div>
															);
														})}
													</div>
												)}
											</div>
										);
									})}
										</>
									)}
								</div>
							</div>
						)}

						{activeAppId === "projects" && (
							<div className="flex-1 min-h-0 flex flex-col">
								<div className="flex items-center justify-between gap-2 px-2 py-1.5">
									<span className="text-xs uppercase tracking-wide text-muted-foreground">
										{locale === "de" ? "Projekte" : "Projects"}
									</span>
									<span className="text-xs text-muted-foreground/50">
										({projectSummaries.length})
									</span>
								</div>
								<div className="flex-1 overflow-y-auto overflow-x-hidden space-y-2 px-1">
									{projectSummaries.length === 0 ? (
										<div className="text-sm text-muted-foreground/60 text-center py-6">
											{locale === "de"
												? "Noch keine Projekte"
												: "No projects yet"}
										</div>
									) : (
										projectSummaries.map((project) => {
											const lastActiveLabel = project.lastActive
												? formatSessionDate(project.lastActive)
												: locale === "de"
													? "Nie"
													: "Never";
											const defaultAgent = projectDefaultAgents[project.key];
											return (
												<div
													key={project.key}
													className={cn(
														"border rounded-md overflow-hidden",
														selectedProjectKey === project.key
															? "border-primary"
															: "border-sidebar-border",
													)}
												>
													<button
														type="button"
														onClick={() => handleProjectSelect(project.key)}
														className="w-full px-3 py-2 text-left hover:bg-sidebar-accent transition-colors"
													>
														<div className="flex items-center gap-2">
															<FolderKanban className="w-4 h-4 text-primary/80" />
															<span className="text-sm font-medium truncate">
																{project.name}
															</span>
														</div>
														<div className="text-xs text-muted-foreground/60 mt-1">
															{project.sessionCount}{" "}
															{locale === "de" ? "Chats" : "chats"} ·{" "}
															{lastActiveLabel}
														</div>
														<div className="text-xs text-muted-foreground/60 mt-0.5">
															{locale === "de"
																? "Standard-Agent"
																: "Default agent"}
															: {defaultAgent || "-"}
														</div>
													</button>
													<div className="px-3 pb-2">
														<select
															value={defaultAgent || ""}
															onChange={(e) =>
																handleProjectDefaultAgentChange(
																	project.key,
																	e.target.value,
																)
															}
															className="w-full text-xs bg-sidebar-accent/50 border border-sidebar-border rounded px-2 py-1"
														>
															<option value="">
																{locale === "de"
																	? "Standard-Agent setzen"
																	: "Set default agent"}
															</option>
															{availableAgents.map((agent) => (
																<option key={agent.id} value={agent.id}>
																	{agent.name || agent.id}
																</option>
															))}
														</select>
													</div>
												</div>
											);
										})
									)}
								</div>
							</div>
						)}

						{activeAppId === "agents" && (
							<div className="flex-1 min-h-0 flex flex-col">
								<div className="flex items-center justify-between gap-2 px-2 py-1.5">
									<span className="text-xs uppercase tracking-wide text-muted-foreground">
										{locale === "de" ? "Agenten" : "Agents"}
									</span>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={() => setMobileMenuOpen(false)}
										className="text-xs"
									>
										{locale === "de" ? "Erstellen" : "Create"}
									</Button>
								</div>
								<div className="flex-1 overflow-y-auto overflow-x-hidden space-y-2 px-1">
									{availableAgents.length === 0 ? (
										<div className="text-sm text-muted-foreground/60 text-center py-6">
											{locale === "de"
												? "Keine Agenten gefunden"
												: "No agents found"}
										</div>
									) : (
										availableAgents.map((agent) => (
											<div
												key={agent.id}
												className="border border-sidebar-border rounded-md px-3 py-2 text-left"
											>
												<div className="text-sm font-medium">
													{agent.name || agent.id}
												</div>
												<div className="text-xs text-muted-foreground/60">
													{agent.model?.providerID
														? `${agent.model.providerID}/${agent.model.modelID ?? ""}`
														: agent.id}
												</div>
											</div>
										))
									)}
								</div>
							</div>
						)}
					</nav>

					<div className="w-full px-4 pb-2">
						<div className="h-px w-full bg-primary/50 mb-2" />
						<div className="flex items-center justify-center gap-3">
							<Button
								type="button"
								variant="ghost"
								size="icon"
								rounded="full"
								onClick={() => handleMobileToggleClick("dashboard")}
								aria-label="Dashboard"
								className={cn(
									"hover:bg-sidebar-accent",
									activeAppId === "dashboard"
										? "text-primary"
										: "text-muted-foreground hover:text-primary",
								)}
							>
								<LayoutDashboard className="w-5 h-5" />
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								rounded="full"
								onClick={() => handleMobileToggleClick("settings")}
								aria-label="Settings"
								className={cn(
									"hover:bg-sidebar-accent",
									activeAppId === "settings"
										? "text-primary"
										: "text-muted-foreground hover:text-primary",
								)}
							>
								<Settings className="w-5 h-5" />
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								rounded="full"
								onClick={() => handleMobileToggleClick("admin")}
								aria-label="Admin"
								className={cn(
									"hover:bg-sidebar-accent",
									activeAppId === "admin"
										? "text-primary"
										: "text-muted-foreground hover:text-primary",
								)}
							>
								<Shield className="w-5 h-5" />
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								rounded="full"
								onClick={() => {
									toggleLocale();
									setMobileMenuOpen(false);
								}}
								aria-label="Sprache wechseln"
								className="text-muted-foreground hover:text-primary hover:bg-sidebar-accent"
							>
								<Globe2 className="w-5 h-5" />
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								rounded="full"
								onClick={() => {
									toggleTheme();
									setMobileMenuOpen(false);
								}}
								aria-pressed={isDark}
								className="text-muted-foreground hover:text-primary hover:bg-sidebar-accent"
							>
								{isDark ? (
									<SunMedium className="w-5 h-5" />
								) : (
									<MoonStar className="w-5 h-5" />
								)}
							</Button>
						</div>
					</div>
				</div>
			)}

			{/* Desktop sidebar */}
			<aside
				className={`fixed inset-y-0 left-0 flex-col transition-all duration-200 z-40 hidden md:flex ${
					sidebarCollapsed
						? "w-[4.5rem] items-center"
						: "w-[16.25rem] items-center"
				}`}
				style={{ backgroundColor: sidebarBg }}
			>
				<div
					className={cn(
						"h-20 w-full flex items-center px-4",
						sidebarCollapsed ? "justify-center" : "justify-center relative",
					)}
				>
					{!sidebarCollapsed && (
						<img
							src={
								isDark ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"
							}
							alt="OCTO"
							width={200}
							height={60}
							className="h-14 w-auto object-contain"
						/>
					)}
					<Button
						type="button"
						variant="ghost"
						size="icon"
						aria-label="Sidebar umschalten"
						onClick={() => setSidebarCollapsed((prev) => !prev)}
						className={cn(
							"text-muted-foreground hover:text-primary",
							!sidebarCollapsed && "absolute right-3",
						)}
					>
						{sidebarCollapsed ? (
							<PanelRightClose className="w-4 h-4" />
						) : (
							<PanelLeftClose className="w-4 h-4" />
						)}
					</Button>
				</div>
				{sidebarCollapsed && (
					<div className="w-full px-2">
						<div className="h-px w-full bg-primary/50" />
					</div>
				)}
				{/* Session history list - always visible when not collapsed */}
				{!sidebarCollapsed && chatHistory.length > 0 && (
					<>
						<div className="w-full px-4">
							<div className="h-px w-full bg-primary/50" />
						</div>
						<div className="w-full px-1.5 mt-2 flex-1 min-h-0 flex flex-col overflow-x-hidden">
							{/* Sticky header section - Search, Main Chat, Sessions header */}
							<div className="flex-shrink-0 space-y-0.5">
								{/* Search input with mode dropdown */}
								<div className="relative mb-2 px-1">
									{/* Search mode dropdown on left */}
									<DropdownMenu>
										<DropdownMenuTrigger asChild>
											<button
												type="button"
												className={cn(
													"absolute left-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] font-medium transition-colors",
													searchMode === "messages"
														? "bg-primary/20 text-primary"
														: "text-muted-foreground hover:text-foreground hover:bg-sidebar-accent",
												)}
												title="Ctrl+Shift+F"
											>
												{searchMode === "messages" ? (
													<MessageSquare className="w-3 h-3" />
												) : (
													<Search className="w-3 h-3" />
												)}
												<ChevronDown className="w-2.5 h-2.5" />
											</button>
										</DropdownMenuTrigger>
										<DropdownMenuContent align="start" className="w-48">
											<DropdownMenuItem
												onClick={() => setSearchMode("sessions")}
												className={cn(searchMode === "sessions" && "bg-accent")}
											>
												<Search className="w-3.5 h-3.5 mr-2" />
												{locale === "de"
													? "Sitzungen filtern"
													: "Filter sessions"}
											</DropdownMenuItem>
											<DropdownMenuItem
												onClick={() => setSearchMode("messages")}
												className={cn(searchMode === "messages" && "bg-accent")}
											>
												<MessageSquare className="w-3.5 h-3.5 mr-2" />
												{locale === "de"
													? "Nachrichten suchen"
													: "Search messages"}
											</DropdownMenuItem>
											{searchMode === "messages" && (
												<>
													<DropdownMenuSeparator />
													<DropdownMenuItem
														onClick={() => setAgentFilter("all")}
														className={cn(agentFilter === "all" && "bg-accent")}
													>
														{locale === "de" ? "Alle Agenten" : "All agents"}
													</DropdownMenuItem>
													<DropdownMenuItem
														onClick={() => setAgentFilter("opencode")}
														className={cn(
															agentFilter === "opencode" && "bg-accent",
														)}
													>
														{locale === "de" ? "Nur OpenCode" : "OpenCode only"}
													</DropdownMenuItem>
													<DropdownMenuItem
														onClick={() => setAgentFilter("pi_agent")}
														className={cn(
															agentFilter === "pi_agent" && "bg-accent",
														)}
													>
														{locale === "de"
															? "Nur Main Chat"
															: "Main Chat only"}
													</DropdownMenuItem>
												</>
											)}
										</DropdownMenuContent>
									</DropdownMenu>
									<input
										type="text"
										placeholder={
											searchMode === "messages"
												? locale === "de"
													? "Nachrichten durchsuchen..."
													: "Search messages..."
												: locale === "de"
													? "Suchen..."
													: "Search..."
										}
										value={sessionSearch}
										onChange={(e) => setSessionSearch(e.target.value)}
										className="w-full pl-12 pr-8 py-1.5 text-xs bg-sidebar-accent/50 border border-sidebar-border rounded placeholder:text-muted-foreground/50 focus:outline-none focus:border-primary/50"
									/>
									<div className="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5">
										{sessionSearch && (
											<button
												type="button"
												onClick={() => {
													setSessionSearch("");
													setSearchMode("sessions");
												}}
												className="p-1 text-muted-foreground hover:text-foreground"
												title={
													locale === "de" ? "Suche beenden" : "Close search"
												}
											>
												<X className="w-3 h-3" />
											</button>
										)}
									</div>
								</div>
								{/* Main Chat */}
								<div className="mb-2 pb-2 px-2 pt-2">
									<MainChatEntry
										isSelected={mainChatActive}
										activeSessionId={
											mainChatActive ? mainChatCurrentSessionId : null
										}
										onSelect={handleMainChatSelect}
										onSessionSelect={handleMainChatSessionSelect}
										onNewSession={handleMainChatNewSession}
										locale={locale}
									/>
								</div>
								{/* Sessions header - between search and chat list */}
								<div className="flex items-center justify-between gap-2 py-1.5 px-1">
									<div className="flex items-center gap-2">
										<span className="text-xs uppercase tracking-wide text-muted-foreground">
											{locale === "de" ? "Sitzungen" : "Sessions"}
										</span>
										<span className="text-xs text-muted-foreground/50">
											({filteredSessions.length}
											{deferredSearch ? `/${chatHistory.length}` : ""})
										</span>
									</div>
									<div className="flex items-center gap-1">
										<button
											type="button"
											onClick={handleNewChat}
											className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded"
											title={
												locale === "de" ? "Neue Sitzung" : "New session"
											}
										>
											<Plus className="w-4 h-4" />
										</button>
										<button
											type="button"
											onClick={() => setNewProjectDialogOpen(true)}
											className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded"
											title={
												locale === "de" ? "Neues Projekt" : "New project"
											}
										>
											<FolderPlus className="w-4 h-4" />
										</button>
										{selectedProjectLabel && (
											<button
												type="button"
												onClick={handleProjectClear}
												className="flex items-center gap-1 text-[10px] text-muted-foreground/70 hover:text-foreground"
											>
												<X className="w-3 h-3" />
												{selectedProjectLabel}
											</button>
										)}
										{/* Sort dropdown */}
										<DropdownMenu>
											<DropdownMenuTrigger asChild>
												<button
													type="button"
													className="p-1 text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded"
													title={locale === "de" ? "Sortieren" : "Sort"}
												>
													{projectSortAsc ? (
														<ArrowUp className="w-3 h-3" />
													) : (
														<ArrowDown className="w-3 h-3" />
													)}
												</button>
											</DropdownMenuTrigger>
											<DropdownMenuContent align="end" className="w-36">
												<DropdownMenuItem
													onClick={() => setProjectSortBy("date")}
													className={cn(
														projectSortBy === "date" && "bg-accent",
													)}
												>
													<Clock className="w-3.5 h-3.5 mr-2" />
													{locale === "de" ? "Datum" : "Date"}
												</DropdownMenuItem>
												<DropdownMenuItem
													onClick={() => setProjectSortBy("name")}
													className={cn(
														projectSortBy === "name" && "bg-accent",
													)}
												>
													<ArrowUpDown className="w-3.5 h-3.5 mr-2" />
													{locale === "de" ? "Name" : "Name"}
												</DropdownMenuItem>
												<DropdownMenuItem
													onClick={() => setProjectSortBy("sessions")}
													className={cn(
														projectSortBy === "sessions" && "bg-accent",
													)}
												>
													<MessageSquare className="w-3.5 h-3.5 mr-2" />
													{locale === "de" ? "Anzahl" : "Count"}
												</DropdownMenuItem>
												<DropdownMenuSeparator />
												<DropdownMenuItem
													onClick={() => setProjectSortAsc(!projectSortAsc)}
												>
													{projectSortAsc ? (
														<>
															<ArrowDown className="w-3.5 h-3.5 mr-2" />
															{locale === "de" ? "Absteigend" : "Descending"}
														</>
													) : (
														<>
															<ArrowUp className="w-3.5 h-3.5 mr-2" />
															{locale === "de" ? "Aufsteigend" : "Ascending"}
														</>
													)}
												</DropdownMenuItem>
											</DropdownMenuContent>
										</DropdownMenu>
									</div>
								</div>
							</div>
							{/* Scrollable chat list - grouped by project OR search results */}
							<div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden space-y-1">
								{/* Message search results (when in messages mode with query) */}
								{searchMode === "messages" && sessionSearch.trim() ? (
									<SearchResults
										query={sessionSearch}
										agentFilter={agentFilter}
										locale={locale}
										onResultClick={handleSearchResultClick}
										extraHits={messageSearchExtraHits}
									/>
								) : (
									<>
										{filteredSessions.length === 0 && deferredSearch && (
											<div className="text-xs text-muted-foreground/50 text-center py-4">
												{locale === "de" ? "Keine Ergebnisse" : "No results"}
											</div>
										)}
										{sessionsByProject.map((project) => {
											// Auto-expand all when searching
											const isProjectExpanded =
												deferredSearch || expandedProjects.has(project.key);
											const isProjectPinned = pinnedProjects.includes(
												project.key,
											);
											return (
												<div
													key={project.key}
													className="border-b border-sidebar-border/50 last:border-b-0"
												>
													{/* Project header */}
													<ContextMenu>
														<ContextMenuTrigger className="contents">
															<div className="flex items-center gap-1 px-1 py-1.5 group">
																<button
																	type="button"
																	onClick={() =>
																		toggleProjectExpanded(project.key)
																	}
																	className="flex-1 flex items-center gap-1.5 text-left hover:bg-sidebar-accent/50 px-1 py-0.5 -mx-1"
																>
																	{isProjectExpanded ? (
																		<ChevronDown className="w-3 h-3 text-muted-foreground flex-shrink-0" />
																	) : (
																		<ChevronRight className="w-3 h-3 text-muted-foreground flex-shrink-0" />
																	)}
																	{isProjectPinned && (
																		<Pin className="w-3 h-3 text-primary/70 flex-shrink-0" />
																	)}
																	<FolderKanban className="w-3.5 h-3.5 text-primary/70 flex-shrink-0" />
																	<span className="text-xs font-medium text-foreground truncate">
																		{project.name}
																	</span>
																	<span className="text-[10px] text-muted-foreground">
																		({project.sessions.length})
																	</span>
																</button>
																{project.directory ? (
																	<button
																		type="button"
																		onClick={() =>
																			handleNewChatInProject(project.directory)
																		}
																		className="p-1 text-muted-foreground hover:text-primary hover:bg-sidebar-accent opacity-0 group-hover:opacity-100 transition-opacity"
																		title={
																			locale === "de"
																				? "Neuer Chat in diesem Projekt"
																				: "New chat in this project"
																		}
																	>
																		<Plus className="w-3 h-3" />
																	</button>
																) : null}
															</div>
														</ContextMenuTrigger>
														<ContextMenuContent>
															{project.directory && (
																<>
																	<ContextMenuItem
																		onClick={() =>
																			handleNewChatInProject(project.directory)
																		}
																	>
																		<Plus className="w-4 h-4 mr-2" />
																		{locale === "de"
																			? "Neue Sitzung"
																			: "New Session"}
																	</ContextMenuItem>
																	<ContextMenuSeparator />
																</>
															)}
															<ContextMenuItem
																onClick={() => handlePinProject(project.key)}
															>
																<Pin className="w-4 h-4 mr-2" />
																{isProjectPinned
																	? locale === "de"
																		? "Lospinnen"
																		: "Unpin"
																	: locale === "de"
																		? "Anpinnen"
																		: "Pin"}
															</ContextMenuItem>
															<ContextMenuItem
																onClick={() =>
																	handleRenameProject(project.key, project.name)
																}
															>
																<Pencil className="w-4 h-4 mr-2" />
																{locale === "de" ? "Umbenennen" : "Rename"}
															</ContextMenuItem>
															<ContextMenuSeparator />
															<ContextMenuItem
																variant="destructive"
																onClick={() =>
																	handleDeleteProject(project.key, project.name)
																}
															>
																<Trash2 className="w-4 h-4 mr-2" />
																{locale === "de" ? "Loschen" : "Delete"} (
																{project.sessions.length}{" "}
																{project.sessions.length === 1
																	? "chat"
																	: "chats"}
																)
															</ContextMenuItem>
														</ContextMenuContent>
													</ContextMenu>
													{/* Project sessions */}
													{isProjectExpanded && (
														<div className="space-y-0.5 pb-1">
															{project.sessions.map((session) => {
																const isSelected =
																	selectedChatSessionId === session.id;
																const children =
																	sessionHierarchy.childSessionsByParent.get(
																		session.id,
																	) || [];
																const hasChildren = children.length > 0;
																const isExpanded = expandedSessions.has(
																	session.id,
																);
																const readableId = generateReadableId(
																	session.id,
																);
																const formattedDate = session.updated_at
																	? formatSessionDate(session.updated_at)
																	: null;
																return (
																	<div key={session.id} className="ml-3">
																		<ContextMenu>
																			<ContextMenuTrigger className="contents">
																				<div
																					className={cn(
																						"w-full px-2 py-1 text-left transition-colors flex items-start gap-1.5 cursor-pointer",
																						isSelected
																							? "bg-primary/15 border border-primary text-foreground"
																							: "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
																					)}
																				>
																					{hasChildren ? (
																						<button
																							type="button"
																							onClick={() =>
																								toggleSessionExpanded(
																									session.id,
																								)
																							}
																							className="mt-0.5 p-0.5 hover:bg-muted flex-shrink-0 cursor-pointer"
																						>
																							{isExpanded ? (
																								<ChevronDown className="w-3 h-3" />
																							) : (
																								<ChevronRight className="w-3 h-3" />
																							)}
																						</button>
																					) : (
																						<MessageSquare className="w-3 h-3 mt-0.5 flex-shrink-0 text-primary/70" />
																					)}
																					<button
																						type="button"
																						onClick={() =>
																							handleSessionClick(session.id)
																						}
																						className="flex-1 min-w-0 text-left"
																					>
																						<div className="flex items-center gap-1">
																							{pinnedSessions.has(
																								session.id,
																							) && (
																								<Pin className="w-3 h-3 flex-shrink-0 text-primary/70" />
																							)}
																							<span className="text-xs truncate font-medium">
																								{session.title || "Untitled"}
																							</span>
																							{hasChildren && (
																								<span className="text-[10px] text-primary/70">
																									({children.length})
																								</span>
																							)}
																							{busySessions.has(session.id) && (
																								<Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
																							)}
																						</div>
																						{formattedDate && (
																							<div className="text-[9px] text-muted-foreground mt-0.5">
																								{formattedDate}
																							</div>
																						)}
																					</button>
																				</div>
																			</ContextMenuTrigger>
																			<ContextMenuContent>
																				<ContextMenuItem
																					onClick={() => {
																						navigator.clipboard.writeText(
																							readableId,
																						);
																					}}
																				>
																					<Copy className="w-4 h-4 mr-2" />
																					{readableId}
																				</ContextMenuItem>
																				<ContextMenuItem
																					onClick={() => {
																						navigator.clipboard.writeText(
																							session.id,
																						);
																					}}
																				>
																					<Copy className="w-4 h-4 mr-2" />
																					{session.id.slice(0, 16)}...
																				</ContextMenuItem>
																				<ContextMenuSeparator />
																				<ContextMenuItem
																					onClick={() =>
																						handlePinSession(session.id)
																					}
																				>
																					<Pin className="w-4 h-4 mr-2" />
																					{pinnedSessions.has(session.id)
																						? locale === "de"
																							? "Lospinnen"
																							: "Unpin"
																						: locale === "de"
																							? "Anpinnen"
																							: "Pin"}
																				</ContextMenuItem>
																				<ContextMenuItem
																					onClick={() =>
																						handleRenameSession(session.id)
																					}
																				>
																					<Pencil className="w-4 h-4 mr-2" />
																					{locale === "de"
																						? "Umbenennen"
																						: "Rename"}
																				</ContextMenuItem>
																				<ContextMenuSeparator />
																				<ContextMenuItem
																					variant="destructive"
																					onClick={() =>
																						handleDeleteSession(session.id)
																					}
																				>
																					<Trash2 className="w-4 h-4 mr-2" />
																					{locale === "de"
																						? "Loschen"
																						: "Delete"}
																				</ContextMenuItem>
																			</ContextMenuContent>
																		</ContextMenu>
																		{/* Child sessions (subagents) */}
																		{hasChildren && isExpanded && (
																			<div className="ml-4 border-l border-muted pl-2 space-y-0.5 mt-0.5">
																				{children.map((child) => {
																					const isChildSelected =
																						selectedChatSessionId === child.id;
																					const childReadableId =
																						generateReadableId(child.id);
																					const childFormattedDate =
																						child.updated_at
																							? formatSessionDate(
																									child.updated_at,
																								)
																							: null;
																					return (
																						<ContextMenu key={child.id}>
																							<ContextMenuTrigger className="contents">
																								<button
																									type="button"
																									onClick={() =>
																										handleSessionClick(child.id)
																									}
																									className={cn(
																										"w-full px-2 py-1 text-left transition-colors text-xs",
																										isChildSelected
																											? "bg-primary/15 border border-primary text-foreground"
																											: "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
																									)}
																								>
																									<div className="flex items-center gap-1">
																										<Bot className="w-3 h-3 flex-shrink-0 text-primary/70" />
																										<span className="truncate font-medium">
																											{child.title ||
																												"Subagent"}
																										</span>
																										{busySessions.has(
																											child.id,
																										) && (
																											<Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
																										)}
																									</div>
																									{childFormattedDate && (
																										<div className="text-[9px] text-muted-foreground mt-0.5 ml-4">
																											{childFormattedDate}
																										</div>
																									)}
																								</button>
																							</ContextMenuTrigger>
																							<ContextMenuContent>
																								<ContextMenuItem
																									onClick={() => {
																										navigator.clipboard.writeText(
																											childReadableId,
																										);
																									}}
																								>
																									<Copy className="w-4 h-4 mr-2" />
																									{childReadableId}
																								</ContextMenuItem>
																								<ContextMenuItem
																									onClick={() => {
																										navigator.clipboard.writeText(
																											child.id,
																										);
																									}}
																								>
																									<Copy className="w-4 h-4 mr-2" />
																									{child.id.slice(0, 16)}...
																								</ContextMenuItem>
																								<ContextMenuSeparator />
																								<ContextMenuItem
																									onClick={() =>
																										handleRenameSession(
																											child.id,
																										)
																									}
																								>
																									<Pencil className="w-4 h-4 mr-2" />
																									{locale === "de"
																										? "Umbenennen"
																										: "Rename"}
																								</ContextMenuItem>
																								<ContextMenuSeparator />
																								<ContextMenuItem
																									variant="destructive"
																									onClick={() =>
																										handleDeleteSession(
																											child.id,
																										)
																									}
																								>
																									<Trash2 className="w-4 h-4 mr-2" />
																									{locale === "de"
																										? "Loschen"
																										: "Delete"}
																								</ContextMenuItem>
																							</ContextMenuContent>
																						</ContextMenu>
																					);
																				})}
																			</div>
																		)}
																	</div>
																);
															})}
														</div>
													)}
												</div>
											);
										})}
									</>
								)}
							</div>
						</div>
					</>
				)}

				{activeAppId === "projects" && !sidebarCollapsed && (
					<div className="w-full px-2 mt-3 flex-1 min-h-0 flex flex-col">
						<div className="flex items-center justify-between gap-2 py-1.5 px-1 border-t border-sidebar-border">
							<span className="text-xs uppercase tracking-wide text-muted-foreground">
								{locale === "de" ? "Projekte" : "Projects"}
							</span>
							<span className="text-xs text-muted-foreground/50">
								({projectSummaries.length})
							</span>
						</div>
						<div className="flex-1 overflow-y-auto overflow-x-hidden space-y-2 px-1">
							{projectSummaries.length === 0 ? (
								<div className="text-xs text-muted-foreground/60 text-center py-4">
									{locale === "de" ? "Noch keine Projekte" : "No projects yet"}
								</div>
							) : (
								projectSummaries.map((project) => {
									const lastActiveLabel = project.lastActive
										? formatSessionDate(project.lastActive)
										: locale === "de"
											? "Nie"
											: "Never";
									const defaultAgent = projectDefaultAgents[project.key];
									return (
										<div
											key={project.key}
											className={cn(
												"border rounded-md overflow-hidden",
												selectedProjectKey === project.key
													? "border-primary"
													: "border-sidebar-border",
											)}
										>
											<button
												type="button"
												onClick={() => handleProjectSelect(project.key)}
												className="w-full px-3 py-2 text-left hover:bg-sidebar-accent transition-colors"
											>
												<div className="flex items-center gap-2">
													<FolderKanban className="w-4 h-4 text-primary/80" />
													<span className="text-sm font-medium truncate">
														{project.name}
													</span>
												</div>
												<div className="text-xs text-muted-foreground/60 mt-1">
													{project.sessionCount}{" "}
													{locale === "de" ? "Chats" : "chats"} ·{" "}
													{lastActiveLabel}
												</div>
												<div className="text-xs text-muted-foreground/60 mt-0.5">
													{locale === "de" ? "Standard-Agent" : "Default agent"}
													: {defaultAgent || "-"}
												</div>
											</button>
											<div className="px-3 pb-2">
												<select
													value={defaultAgent || ""}
													onChange={(e) =>
														handleProjectDefaultAgentChange(
															project.key,
															e.target.value,
														)
													}
													className="w-full text-xs bg-sidebar-accent/50 border border-sidebar-border rounded px-2 py-1"
												>
													<option value="">
														{locale === "de"
															? "Standard-Agent setzen"
															: "Set default agent"}
													</option>
													{availableAgents.map((agent) => (
														<option key={agent.id} value={agent.id}>
															{agent.name || agent.id}
														</option>
													))}
												</select>
											</div>
										</div>
									);
								})
							)}
						</div>
					</div>
				)}

				{activeAppId === "agents" && !sidebarCollapsed && (
					<div className="w-full px-2 mt-3 flex-1 min-h-0 flex flex-col">
						<div className="flex items-center justify-between gap-2 py-1.5 px-1 border-t border-sidebar-border">
							<span className="text-xs uppercase tracking-wide text-muted-foreground">
								{locale === "de" ? "Agenten" : "Agents"}
							</span>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="text-xs"
								onClick={() => activateApp("agents")}
							>
								{locale === "de" ? "Erstellen" : "Create"}
							</Button>
						</div>
						<div className="flex-1 overflow-y-auto overflow-x-hidden space-y-2 px-1">
							{availableAgents.length === 0 ? (
								<div className="text-xs text-muted-foreground/60 text-center py-4">
									{locale === "de"
										? "Keine Agenten gefunden"
										: "No agents found"}
								</div>
							) : (
								availableAgents.map((agent) => (
									<div
										key={agent.id}
										className="border border-sidebar-border rounded-md px-3 py-2 text-left"
									>
										<div className="text-sm font-medium">
											{agent.name || agent.id}
										</div>
										<div className="text-xs text-muted-foreground/60">
											{agent.model?.providerID
												? `${agent.model.providerID}/${agent.model.modelID ?? ""}`
												: agent.id}
										</div>
									</div>
								))
							)}
						</div>
					</div>
				)}

				{/* Collapsed session indicator - always visible when collapsed */}
				{sidebarCollapsed &&
					(chatHistory.length > 0 || opencodeSessions.length > 0) && (
						<div className="w-full px-2 mt-4">
							<div className="pt-2">
								<button
									type="button"
									onClick={() => setSidebarCollapsed(false)}
									className="w-full p-2 text-muted-foreground hover:text-foreground transition-colors"
									title={locale === "de" ? "Verlauf anzeigen" : "Show history"}
								>
									<Clock className="w-4 h-4 mx-auto" />
								</button>
							</div>
						</div>
					)}

				<div
					className={`w-full ${sidebarCollapsed ? "px-2 pb-3" : "px-4 pb-4"} mt-auto pt-3`}
				>
					<div className="h-px w-full bg-primary/50 mb-3" />
					<div
						className={`flex items-center ${sidebarCollapsed ? "flex-col gap-2" : "justify-center gap-2"}`}
					>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							rounded="full"
							onClick={() => toggleApp("dashboard")}
							aria-label="Dashboard"
							className="w-9 h-9 flex items-center justify-center transition-colors"
							style={{
								backgroundColor:
									activeAppId === "dashboard" ? navActiveBg : navIdle,
								border:
									activeAppId === "dashboard"
										? `1px solid ${navActiveBorder}`
										: "1px solid transparent",
								color: activeAppId === "dashboard" ? navActiveText : navText,
							}}
							onMouseEnter={(e) => {
								if (activeAppId !== "dashboard") {
									e.currentTarget.style.backgroundColor = sidebarHover;
									e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
								}
							}}
							onMouseLeave={(e) => {
								if (activeAppId !== "dashboard") {
									e.currentTarget.style.backgroundColor = navIdle;
									e.currentTarget.style.border = "1px solid transparent";
								}
							}}
						>
							<LayoutDashboard className="w-4 h-4" />
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							rounded="full"
							onClick={() => toggleApp("settings")}
							aria-label="Settings"
							className="w-9 h-9 flex items-center justify-center transition-colors"
							style={{
								backgroundColor:
									activeAppId === "settings" ? navActiveBg : navIdle,
								border:
									activeAppId === "settings"
										? `1px solid ${navActiveBorder}`
										: "1px solid transparent",
								color: activeAppId === "settings" ? navActiveText : navText,
							}}
							onMouseEnter={(e) => {
								if (activeAppId !== "settings") {
									e.currentTarget.style.backgroundColor = sidebarHover;
									e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
								}
							}}
							onMouseLeave={(e) => {
								if (activeAppId !== "settings") {
									e.currentTarget.style.backgroundColor = navIdle;
									e.currentTarget.style.border = "1px solid transparent";
								}
							}}
						>
							<Settings className="w-4 h-4" />
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							rounded="full"
							onClick={() => toggleApp("admin")}
							aria-label="Admin"
							className="w-9 h-9 flex items-center justify-center transition-colors"
							style={{
								backgroundColor:
									activeAppId === "admin" ? navActiveBg : navIdle,
								border:
									activeAppId === "admin"
										? `1px solid ${navActiveBorder}`
										: "1px solid transparent",
								color: activeAppId === "admin" ? navActiveText : navText,
							}}
							onMouseEnter={(e) => {
								if (activeAppId !== "admin") {
									e.currentTarget.style.backgroundColor = sidebarHover;
									e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
								}
							}}
							onMouseLeave={(e) => {
								if (activeAppId !== "admin") {
									e.currentTarget.style.backgroundColor = navIdle;
									e.currentTarget.style.border = "1px solid transparent";
								}
							}}
						>
							<Shield className="w-4 h-4" />
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							rounded="full"
							onClick={toggleLocale}
							aria-label="Sprache wechseln"
							className="w-9 h-9 flex items-center justify-center transition-colors"
							style={{
								backgroundColor: navIdle,
								border: "1px solid transparent",
								color: navText,
							}}
							onMouseEnter={(e) => {
								e.currentTarget.style.backgroundColor = sidebarHover;
								e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
							}}
							onMouseLeave={(e) => {
								e.currentTarget.style.backgroundColor = navIdle;
								e.currentTarget.style.border = "1px solid transparent";
							}}
						>
							<Globe2 className="w-4 h-4" />
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							rounded="full"
							onClick={toggleTheme}
							aria-pressed={isDark}
							className="w-9 h-9 flex items-center justify-center transition-colors"
							style={{
								backgroundColor: navIdle,
								border: "1px solid transparent",
								color: navText,
							}}
							onMouseEnter={(e) => {
								e.currentTarget.style.backgroundColor = sidebarHover;
								e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
							}}
							onMouseLeave={(e) => {
								e.currentTarget.style.backgroundColor = navIdle;
								e.currentTarget.style.border = "1px solid transparent";
							}}
						>
							{isDark ? (
								<SunMedium className="w-4 h-4" />
							) : (
								<MoonStar className="w-4 h-4" />
							)}
						</Button>
					</div>
				</div>
			</aside>

			{/* Main content */}
			<div
				className="flex-1 flex flex-col min-h-0 overflow-hidden"
				style={{ backgroundColor: shellBg }}
			>
				<div
					className={`flex-1 min-h-0 overflow-hidden pt-[calc(3.5rem+env(safe-area-inset-top))] md:pt-0 transition-all duration-200 flex flex-col ${
						sidebarCollapsed ? "md:pl-[4.5rem]" : "md:pl-[16.25rem]"
					}`}
				>
					<div className="flex-1 min-h-0 w-full pb-0 md:pb-0">
						{ActiveComponent ? <ActiveComponent /> : <EmptyState />}
					</div>
					{/* Status bar */}
					<div className="flex-shrink-0">
						<StatusBar />
					</div>
				</div>
			</div>

			{/* Loading bar */}
			{barVisible && (
				<div className="fixed left-0 top-0 z-[100] w-full pointer-events-none">
					<div
						style={{
							height: "2px",
							width: `${barWidth}%`,
							maxWidth: "100%",
							backgroundColor: "var(--sidebar-ring, #3ba77c)",
							opacity: barFade ? 0 : 1,
							boxShadow: "0 0 12px rgba(59,167,124,0.6)",
							transition: "width 320ms ease, opacity 450ms ease",
						}}
					/>
				</div>
			)}

			{/* Command palette */}
			<CommandPalette
				open={commandPaletteOpen}
				onOpenChange={setCommandPaletteOpen}
			/>

			{/* Delete confirmation dialog */}
			<AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
				<AlertDialogContent>
					<AlertDialogHeader>
						<AlertDialogTitle>
							{locale === "de" ? "Chat loschen?" : "Delete chat?"}
						</AlertDialogTitle>
						<AlertDialogDescription>
							{locale === "de"
								? "Diese Aktion kann nicht ruckgangig gemacht werden. Der Chat wird dauerhaft geloscht."
								: "This action cannot be undone. The chat will be permanently deleted."}
						</AlertDialogDescription>
					</AlertDialogHeader>
					<AlertDialogFooter>
						<AlertDialogCancel>
							{locale === "de" ? "Abbrechen" : "Cancel"}
						</AlertDialogCancel>
						<AlertDialogAction onClick={handleConfirmDelete}>
							{locale === "de" ? "Loschen" : "Delete"}
						</AlertDialogAction>
					</AlertDialogFooter>
				</AlertDialogContent>
			</AlertDialog>

			{/* Rename dialog */}
			<Dialog open={renameDialogOpen} onOpenChange={setRenameDialogOpen}>
				<DialogContent>
					<DialogHeader>
						<DialogTitle>
							{locale === "de" ? "Chat umbenennen" : "Rename chat"}
						</DialogTitle>
						<DialogDescription>
							{locale === "de"
								? "Geben Sie einen neuen Namen fur diesen Chat ein."
								: "Enter a new name for this chat."}
						</DialogDescription>
					</DialogHeader>
					<Input
						value={renameValue}
						onChange={(e) => setRenameValue(e.target.value)}
						placeholder={locale === "de" ? "Chat-Titel" : "Chat title"}
						onKeyDown={(e) => {
							if (e.key === "Enter") {
								handleConfirmRename();
							}
						}}
					/>
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => setRenameDialogOpen(false)}
						>
							{locale === "de" ? "Abbrechen" : "Cancel"}
						</Button>
						<Button type="button" onClick={handleConfirmRename}>
							{locale === "de" ? "Speichern" : "Save"}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>

			{/* Delete project confirmation dialog */}
			<AlertDialog
				open={deleteProjectDialogOpen}
				onOpenChange={setDeleteProjectDialogOpen}
			>
				<AlertDialogContent>
					<AlertDialogHeader>
						<AlertDialogTitle>
							{locale === "de"
								? `Projekt "${targetProjectName}" loschen?`
								: `Delete project "${targetProjectName}"?`}
						</AlertDialogTitle>
						<AlertDialogDescription>
							{locale === "de"
								? "Diese Aktion kann nicht ruckgangig gemacht werden. Alle Chats in diesem Projekt werden dauerhaft geloscht."
								: "This action cannot be undone. All chats in this project will be permanently deleted."}
						</AlertDialogDescription>
					</AlertDialogHeader>
					<AlertDialogFooter>
						<AlertDialogCancel>
							{locale === "de" ? "Abbrechen" : "Cancel"}
						</AlertDialogCancel>
						<AlertDialogAction onClick={handleConfirmDeleteProject}>
							{locale === "de" ? "Loschen" : "Delete"}
						</AlertDialogAction>
					</AlertDialogFooter>
				</AlertDialogContent>
			</AlertDialog>

			{/* Rename project dialog */}
			<Dialog
				open={renameProjectDialogOpen}
				onOpenChange={setRenameProjectDialogOpen}
			>
				<DialogContent>
					<DialogHeader>
						<DialogTitle>
							{locale === "de" ? "Projekt umbenennen" : "Rename project"}
						</DialogTitle>
						<DialogDescription>
							{locale === "de"
								? "Geben Sie einen neuen Namen fur dieses Projekt ein."
								: "Enter a new name for this project."}
						</DialogDescription>
					</DialogHeader>
					<Input
						value={renameProjectValue}
						onChange={(e) => setRenameProjectValue(e.target.value)}
						placeholder={locale === "de" ? "Projektname" : "Project name"}
						onKeyDown={(e) => {
							if (e.key === "Enter") {
								handleConfirmRenameProject();
							}
						}}
					/>
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => setRenameProjectDialogOpen(false)}
						>
							{locale === "de" ? "Abbrechen" : "Cancel"}
						</Button>
						<Button type="button" onClick={handleConfirmRenameProject}>
							{locale === "de" ? "Speichern" : "Save"}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>

			<Dialog
				open={newProjectDialogOpen}
				onOpenChange={handleNewProjectDialogChange}
			>
				<DialogContent className="sm:max-w-xl">
					<DialogHeader>
						<DialogTitle>
							{locale === "de" ? "Neues Projekt" : "New project"}
						</DialogTitle>
						<DialogDescription>
							{locale === "de"
								? "Ein Template auswahlen und ein neues Projekt anlegen."
								: "Pick a template and create a new project."}
						</DialogDescription>
					</DialogHeader>

					<div className="space-y-4">
						<div className="space-y-2">
							<div className="text-xs uppercase text-muted-foreground">
								{locale === "de" ? "Template" : "Template"}
							</div>
							{templatesLoading ? (
								<div className="text-sm text-muted-foreground">
									{locale === "de" ? "Lade Templates..." : "Loading templates..."}
								</div>
							) : templatesError ? (
								<div className="text-sm text-destructive">{templatesError}</div>
							) : !templatesConfigured ? (
								<div className="text-sm text-muted-foreground">
									{locale === "de"
										? "Templates nicht konfiguriert. Setze [templates].repo_path in config.toml."
										: "Templates not configured. Set [templates].repo_path in config.toml."}
								</div>
							) : projectTemplates.length === 0 ? (
								<div className="text-sm text-muted-foreground">
									{locale === "de"
										? "Keine Templates gefunden."
										: "No templates found."}
								</div>
							) : (
								<div className="grid gap-2">
									{projectTemplates.map((template) => {
										const selected = template.path === selectedTemplatePath;
										return (
											<button
												type="button"
												key={template.path}
												onClick={() => setSelectedTemplatePath(template.path)}
												className={cn(
													"flex flex-col gap-1 border rounded px-3 py-2 text-left transition-colors",
													selected
														? "border-primary/70 bg-primary/10"
														: "border-border hover:bg-muted",
												)}
											>
												<span className="text-sm font-medium">
													{template.name}
												</span>
												{template.description && (
													<span className="text-xs text-muted-foreground">
														{template.description}
													</span>
												)}
											</button>
										);
									})}
								</div>
							)}
						</div>

						<div className="space-y-2">
							<div className="text-xs uppercase text-muted-foreground">
								{locale === "de" ? "Projektpfad" : "Project path"}
							</div>
							<Input
								value={newProjectPath}
								onChange={(e) => handleNewProjectPathChange(e.target.value)}
								placeholder={
									locale === "de"
										? "z.B. client-app"
										: "e.g. client-app"
								}
							/>
							<div className="text-xs text-muted-foreground">
								{locale === "de"
									? "Relativ zum Workspace-Ordner."
									: "Relative to the workspace root."}
							</div>
						</div>

						<div className="flex items-center justify-between border border-border rounded px-3 py-2">
							<div className="text-sm">
								{locale === "de" ? "Geteiltes Projekt" : "Shared project"}
							</div>
							<Switch
								checked={newProjectShared}
								onCheckedChange={setNewProjectShared}
							/>
						</div>

						{newProjectError && (
							<div className="text-sm text-destructive">{newProjectError}</div>
						)}
					</div>

					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => handleNewProjectDialogChange(false)}
						>
							{locale === "de" ? "Abbrechen" : "Cancel"}
						</Button>
						<Button
							type="button"
							onClick={handleCreateProjectFromTemplate}
							disabled={newProjectSubmitting || templatesLoading}
						>
							{newProjectSubmitting ? (
								<>
									<Loader2 className="w-4 h-4 mr-2 animate-spin" />
									{locale === "de" ? "Erstelle..." : "Creating..."}
								</>
							) : locale === "de" ? (
								"Projekt erstellen"
							) : (
								"Create project"
							)}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>

			{/* Directory picker dialog */}
			<Dialog
				open={directoryPickerOpen}
				onOpenChange={handleDirectoryPickerOpenChange}
			>
				<DialogContent className="sm:max-w-lg">
					<DialogHeader>
						<DialogTitle>
							{locale === "de"
								? "Arbeitsordner wahlen"
								: "Choose workspace folder"}
						</DialogTitle>
						<DialogDescription>
							{locale === "de"
								? "Wahle ein Projektverzeichnis fur diesen Chat."
								: "Pick a project directory for this chat."}
						</DialogDescription>
					</DialogHeader>

					<div className="space-y-4">
						<div className="flex items-center justify-between text-xs text-muted-foreground">
							<span>{directoryPickerPath}</span>
							{directoryPickerParent && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={() => setDirectoryPickerPath(directoryPickerParent)}
								>
									{locale === "de" ? "Hoch" : "Up"}
								</Button>
							)}
						</div>

						{recentProjects.length > 0 && (
							<div className="space-y-2">
								<div className="text-xs uppercase text-muted-foreground">
									{locale === "de" ? "Zuletzt verwendet" : "Recent projects"}
								</div>
								<div className="grid gap-2 sm:grid-cols-2">
									{recentProjects.map((project) => (
										<button
											type="button"
											key={project.key}
											onClick={() =>
												handleDirectoryConfirm(
													project.key === "workspace" ? "." : project.key,
												)
											}
											className="text-left border border-border rounded px-3 py-2 hover:bg-muted transition-colors"
										>
											<div className="text-sm font-medium truncate">
												{project.name}
											</div>
											<div className="text-xs text-muted-foreground truncate">
												{project.key}
											</div>
										</button>
									))}
								</div>
							</div>
						)}

						<div className="space-y-2">
							<div className="text-xs uppercase text-muted-foreground">
								{locale === "de" ? "Ordner" : "Folders"}
							</div>
							<div className="max-h-56 overflow-y-auto border border-border rounded">
								{directoryPickerLoading ? (
									<div className="p-4 text-sm text-muted-foreground">
										{locale === "de" ? "Lade..." : "Loading..."}
									</div>
								) : directoryPickerEntries.length === 0 ? (
									<div className="p-4 text-sm text-muted-foreground">
										{locale === "de"
											? "Keine Ordner gefunden"
											: "No folders found"}
									</div>
								) : (
									<div className="divide-y divide-border">
										{directoryPickerEntries.map((entry) => (
											<div
												key={entry.path}
												className="flex items-center justify-between px-3 py-2"
											>
												<button
													type="button"
													onClick={() => setDirectoryPickerPath(entry.path)}
													className="text-sm text-left flex-1 truncate hover:text-foreground"
												>
													{entry.name}
												</button>
												<Button
													type="button"
													variant="ghost"
													size="sm"
													onClick={() => handleDirectoryConfirm(entry.path)}
												>
													{locale === "de" ? "Wahlen" : "Select"}
												</Button>
											</div>
										))}
									</div>
								)}
							</div>
						</div>
					</div>

					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => setDirectoryPickerOpen(false)}
						>
							{locale === "de" ? "Abbrechen" : "Cancel"}
						</Button>
						<Button
							type="button"
							onClick={() => handleDirectoryConfirm(directoryPickerPath)}
						>
							{locale === "de" ? "Diesen Ordner nutzen" : "Use this folder"}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>

			{/* Agent picker dialog */}
			<AgentPicker
				open={agentPickerOpen}
				onOpenChange={setAgentPickerOpen}
				onSelect={handleAgentSelect}
				locale={locale}
			/>
		</div>
	);
});

function EmptyState() {
	return (
		<div className="flex items-center justify-center h-full">
			<div className="text-center space-y-2">
				<p className="text-sm text-muted-foreground">No apps registered</p>
				<p className="text-xs text-muted-foreground">
					Register an app in apps/index.ts to get started.
				</p>
			</div>
		</div>
	);
}

export function AppShellRoute() {
	return (
		<AppProvider>
			<AppShell />
		</AppProvider>
	);
}
