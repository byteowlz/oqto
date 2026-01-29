import { AppProvider, useOnboarding } from "@/components/app-context";
import { CommandPalette } from "@/components/command-palette";
import { StatusBar } from "@/components/status-bar";
import { Button } from "@/components/ui/button";
import { useApp } from "@/hooks/use-app";
import { useCommandPalette } from "@/hooks/use-command-palette";
import type { HstrySearchHit } from "@/lib/control-plane-client";
import { getSettingsValues } from "@/lib/control-plane-client";
import { type OpenCodeAgent, fetchAgents } from "@/lib/opencode-client";
import { cn } from "@/lib/utils";
import { Clock, PanelLeftClose, PanelRightClose } from "lucide-react";
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
import { UIControlProvider } from "@/components/contexts/ui-control-context";
import { listMainChatPiSessions } from "@/features/main-chat/api";
import { useMainChatNavigation } from "@/features/main-chat/hooks/useMainChatNavigation";

import {
	DeleteConfirmDialog,
	MobileHeader,
	MobileMenu,
	NewProjectDialog,
	RenameProjectDialog,
	RenameSessionDialog,
	SidebarNav,
	SidebarSessions,
	useProjectActions,
	useSessionData,
	useSessionDialogs,
	useSidebarState,
} from "./app-shell";

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
		selectedChatFromHistory,
		selectedWorkspaceSession,
		opencodeBaseUrl,
		opencodeDirectory,
		ensureOpencodeRunning,
		createOptimisticChatSession,
		clearOptimisticChatSession,
		createNewPiChat,
		deleteChatSession,
		renameChatSession,
		busySessions,
		workspaceSessions,
		setSelectedWorkspaceSessionId,
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
		mainChatNewSessionTrigger,
		requestNewMainChatSession,
		mainChatSessionActivityTrigger,
	} = useApp();

	const location = useLocation();
	const navigate = useNavigate();
	const { setTheme, resolvedTheme } = useTheme();
	const { activateGodmode, state: onboardingState } = useOnboarding();
	const [mounted, setMounted] = useState(false);
	const [selectedProjectKey, setSelectedProjectKey] = useState<string | null>(
		null,
	);
	const [availableAgents, setAvailableAgents] = useState<OpenCodeAgent[]>([]);
	const [sessionSearch, setSessionSearch] = useState("");
	const deferredSearch = useDeferredValue(sessionSearch);
	const [mainChatTitleHits, setMainChatTitleHits] = useState<HstrySearchHit[]>(
		[],
	);

	// Use extracted hooks
	const sidebarState = useSidebarState();
	const projectActions = useProjectActions();
	const sessionDialogs = useSessionDialogs();
	const sessionData = useSessionData({
		chatHistory,
		workspaceDirectories: projectActions.workspaceDirectories,
		locale,
		deferredSearch,
		pinnedSessions: sidebarState.pinnedSessions,
		pinnedProjects: sidebarState.pinnedProjects,
		selectedProjectKey,
		projectSortBy: projectActions.projectSortBy,
		projectSortAsc: projectActions.projectSortAsc,
	});

	const { open: commandPaletteOpen, setOpen: setCommandPaletteOpen } =
		useCommandPalette();

	// Routing
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

	// Route synchronization effects
	useEffect(() => {
		if (matchedAppId && matchedAppId !== activeAppId) {
			if (matchedAppId === "sessions" && virtualApps.has(activeAppId)) return;
			setActiveAppId(matchedAppId);
			if (virtualApps.has(matchedAppId) && sessionsRoute) {
				navigate(sessionsRoute, { replace: true });
			}
			return;
		}
		if (!matchedAppId && location.pathname === "/" && sessionsRoute) {
			navigate(sessionsRoute, { replace: true });
		}
	}, [
		activeAppId,
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
		if (!isMatch) navigate(activeRoute, { replace: true });
	}, [activeAppId, apps, location.pathname, matchedAppId, navigate]);

	// Shell ready state
	const [shellReady, setShellReady] = useState(false);

	useEffect(() => {
		setMounted(true);
	}, []);

	useEffect(() => {
		if (!mounted) return;
		const timer = requestAnimationFrame(() => {
			requestAnimationFrame(() => {
				setShellReady(true);
				document.getElementById("preload")?.remove();
				document.documentElement.removeAttribute("data-preload");
			});
		});
		return () => cancelAnimationFrame(timer);
	}, [mounted]);

	const currentTheme = mounted ? resolvedTheme : "light";
	const isDark = currentTheme === "dark";
	const ActiveComponent = activeApp?.component ?? null;

	// Loading bar
	const [barVisible, setBarVisible] = useState(true);
	const [barWidth, setBarWidth] = useState(0);
	const [barFade, setBarFade] = useState(false);

	// Keyboard shortcuts
	useEffect(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			if (e.key === "g" && (e.metaKey || e.ctrlKey) && e.shiftKey) {
				e.preventDefault();
				if (!onboardingState.completed && !onboardingState.godmode) {
					activateGodmode();
				}
			}
		};
		document.addEventListener("keydown", handleKeyDown);
		return () => document.removeEventListener("keydown", handleKeyDown);
	}, [activateGodmode, onboardingState.completed, onboardingState.godmode]);

	// Main chat title search
	useEffect(() => {
		const query = deferredSearch.trim().toLowerCase();
		if (!query) {
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
	}, [deferredSearch]);

	// Fetch agents
	useEffect(() => {
		if (!opencodeBaseUrl) return;
		fetchAgents(opencodeBaseUrl, { directory: opencodeDirectory })
			.then(setAvailableAgents)
			.catch(() => setAvailableAgents([]));
	}, [opencodeBaseUrl, opencodeDirectory]);

	// Load settings
	useEffect(() => {
		let mounted = true;
		getSettingsValues("octo")
			.then((values) => {
				if (!mounted) return;
				const raw = values["sessions.max_concurrent_sessions"]?.value;
				// Session limit unused but kept for future use
			})
			.catch(() => {});
		return () => {
			mounted = false;
		};
	}, []);

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
		setMobileMenuOpen: sidebarState.setMobileMenuOpen,
		setMainChatWorkspacePath,
		requestNewMainChatSession,
	});

	const messageSearchExtraHits = useMemo(
		() => [...sessionData.sessionTitleHits, ...mainChatTitleHits],
		[sessionData.sessionTitleHits, mainChatTitleHits],
	);

	// Event handlers
	const handleProjectSelect = useCallback(
		(projectKey: string) => {
			setSelectedProjectKey(projectKey);
			setActiveAppId("sessions");
			if (sessionsRoute) navigate(sessionsRoute);
			sidebarState.setMobileMenuOpen(false);
		},
		[navigate, sessionsRoute, setActiveAppId, sidebarState],
	);

	const handleProjectClear = useCallback(() => {
		setSelectedProjectKey(null);
	}, []);

	const handleSessionClick = useCallback(
		(sessionId: string) => {
			setSelectedChatSessionId(sessionId);
			setActiveAppId("sessions");
			if (sessionsRoute) navigate(sessionsRoute);
			sidebarState.setMobileMenuOpen(false);
			setMainChatActive(false);
			setMainChatWorkspacePath(null);

			const selectedSession = chatHistory.find((s) => s.id === sessionId);
			if (selectedSession?.workspace_path) {
				const matchingWorkspaceSession = workspaceSessions.find(
					(ws) => ws.workspace_path === selectedSession.workspace_path,
				);
				if (matchingWorkspaceSession) {
					setSelectedWorkspaceSessionId(matchingWorkspaceSession.id);
				}
			}
		},
		[
			chatHistory,
			navigate,
			sessionsRoute,
			setActiveAppId,
			setMainChatActive,
			setMainChatWorkspacePath,
			setSelectedChatSessionId,
			setSelectedWorkspaceSessionId,
			sidebarState,
			workspaceSessions,
		],
	);

	const handleSearchResultClick = useCallback(
		(hit: HstrySearchHit) => {
			setSessionSearch("");
			const targetMessageId =
				hit.message_id || (hit.line_number ? `line-${hit.line_number}` : null);
			if (targetMessageId) setScrollToMessageId(targetMessageId);

			if (hit.agent === "pi_agent") {
				setActiveAppId("sessions");
				if (sessionsRoute) navigate(sessionsRoute);
				setMainChatActive(true);
				if (hit.session_id) setMainChatCurrentSessionId(hit.session_id);
				if (hit.workspace) setMainChatWorkspacePath(hit.workspace);
			} else if (hit.agent === "opencode" || hit.agent === "claude_code") {
				const sessionId =
					hit.session_id ||
					hit.source_path.match(/ses_[a-zA-Z0-9]+/)?.[0] ||
					"";
				if (sessionId) {
					setSelectedChatSessionId(sessionId);
					setActiveAppId("sessions");
					if (sessionsRoute) navigate(sessionsRoute);
					setMainChatActive(false);
					setMainChatWorkspacePath(null);
				}
			}
			sidebarState.setMobileMenuOpen(false);
		},
		[
			setActiveAppId,
			setMainChatActive,
			setMainChatCurrentSessionId,
			setMainChatWorkspacePath,
			setSelectedChatSessionId,
			setScrollToMessageId,
			navigate,
			sessionsRoute,
			sidebarState,
		],
	);

	const handleNewChat = useCallback(async () => {
		if (mainChatActive) {
			setActiveAppId("sessions");
			requestNewMainChatSession();
			return;
		}

		if (selectedProjectKey) {
			const project = sessionData.projectSummaries.find(
				(p) => p.key === selectedProjectKey,
			);
			if (project?.directory) {
				setActiveAppId("sessions");
				const optimisticId = createOptimisticChatSession(project.directory);
				const created = await createNewPiChat(project.directory, {
					optimisticId,
				});
				if (created) return;
				clearOptimisticChatSession(optimisticId);
			}
		}

		if (selectedWorkspaceSession) {
			setActiveAppId("sessions");
			const workspacePath =
				selectedWorkspaceSession.workspace_path ??
				opencodeDirectory ??
				"global";
			const optimisticId = createOptimisticChatSession(workspacePath);
			const created = await createNewPiChat(workspacePath, { optimisticId });
			if (created) return;
			clearOptimisticChatSession(optimisticId);
			return;
		}

		const currentWorkspacePath = selectedChatFromHistory?.workspace_path;
		if (currentWorkspacePath && currentWorkspacePath !== "global") {
			setActiveAppId("sessions");
			const optimisticId = createOptimisticChatSession(currentWorkspacePath);
			const created = await createNewPiChat(currentWorkspacePath, {
				optimisticId,
			});
			if (created) return;
			clearOptimisticChatSession(optimisticId);
		}

		setActiveAppId("sessions");
		const created = await createNewPiChat("global");
		if (created) return;
		requestNewMainChatSession();
	}, [
		mainChatActive,
		requestNewMainChatSession,
		selectedWorkspaceSession,
		opencodeDirectory,
		selectedChatFromHistory,
		selectedProjectKey,
		sessionData.projectSummaries,
		createNewPiChat,
		createOptimisticChatSession,
		clearOptimisticChatSession,
		setActiveAppId,
	]);

	const handleNewChatInProject = useCallback(
		async (directory: string) => {
			setActiveAppId("sessions");
			sidebarState.setMobileMenuOpen(false);
			const optimisticId = createOptimisticChatSession(directory);
			const created = await createNewPiChat(directory, { optimisticId });
			if (created) return;
			clearOptimisticChatSession(optimisticId);
		},
		[
			createNewPiChat,
			createOptimisticChatSession,
			clearOptimisticChatSession,
			setActiveAppId,
			sidebarState,
		],
	);

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

	// External event listeners
	useEffect(() => {
		if (typeof window === "undefined") return;
		const handleFilter = (event: Event) => {
			const customEvent = event as CustomEvent<string>;
			if (typeof customEvent.detail === "string") {
				setSelectedProjectKey(customEvent.detail);
				setActiveAppId("sessions");
			}
		};
		const handleClear = () => setSelectedProjectKey(null);
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
		window.addEventListener("octo:project-filter-clear", handleClear);
		window.addEventListener(
			"octo:project-default-agent",
			handleDefaultAgent as EventListener,
		);
		return () => {
			window.removeEventListener(
				"octo:project-filter",
				handleFilter as EventListener,
			);
			window.removeEventListener("octo:project-filter-clear", handleClear);
			window.removeEventListener(
				"octo:project-default-agent",
				handleDefaultAgent as EventListener,
			);
		};
	}, [handleProjectDefaultAgentChange, setActiveAppId]);

	// Viewport and loading bar
	useEffect(() => {
		if (typeof window === "undefined") return;

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
		document.documentElement.classList.add("no-transitions");
		setTheme(next);
		requestAnimationFrame(() => {
			requestAnimationFrame(() => {
				document.documentElement.classList.remove("no-transitions");
			});
		});
	};

	const toggleLocale = () => setLocale(locale === "de" ? "en" : "de");

	const activateApp = useCallback(
		(appId: string) => {
			setActiveAppId(appId);
			const route = apps.find((app) => app.id === appId)?.routes?.[0];
			if (!route) return;
			if (virtualApps.has(appId)) {
				if (sessionsRoute) navigate(sessionsRoute);
				return;
			}
			navigate(route);
		},
		[apps, navigate, sessionsRoute, setActiveAppId, virtualApps],
	);

	const toggleApp = useCallback(
		(appId: string) => {
			if (activeAppId === appId) {
				setActiveAppId("sessions");
				if (sessionsRoute) navigate(sessionsRoute);
			} else {
				activateApp(appId);
			}
		},
		[activeAppId, activateApp, navigate, sessionsRoute, setActiveAppId],
	);

	const handleMobileToggleClick = (appId: string) => {
		if (activeAppId === appId) activateApp("sessions");
		else activateApp(appId);
		sidebarState.setMobileMenuOpen(false);
	};

	const sidebarBg = "var(--sidebar, #181b1a)";
	const shellBg = "var(--background)";

	return (
		<UIControlProvider
			sidebarCollapsed={sidebarState.sidebarCollapsed}
			setSidebarCollapsed={sidebarState.setSidebarCollapsed}
			setCommandPaletteOpen={setCommandPaletteOpen}
		>
			<div
				className="flex min-h-screen bg-background text-foreground overflow-hidden transition-opacity duration-300 ease-out"
				style={{
					opacity: shellReady ? 1 : 0,
					height: "var(--app-viewport-height, 100vh)",
				}}
			>
				<MobileHeader
					locale={locale}
					isDark={isDark}
					activeAppId={activeAppId}
					activeApp={activeApp}
					resolveText={resolveText}
					selectedChatFromHistory={selectedChatFromHistory}
					opencodeDirectory={opencodeDirectory}
					mainChatActive={mainChatActive}
					mainChatAssistantName={mainChatAssistantName}
					onMenuOpen={() => sidebarState.setMobileMenuOpen(true)}
					onNewChat={handleNewChat}
				/>

				{sidebarState.mobileMenuOpen && (
					<MobileMenu
						locale={locale}
						isDark={isDark}
						activeAppId={activeAppId}
						chatHistory={chatHistory}
						sessionHierarchy={sessionData.sessionHierarchy}
						sessionsByProject={sessionData.sessionsByProject}
						filteredSessions={sessionData.filteredSessions}
						selectedChatSessionId={selectedChatSessionId}
						selectedProjectKey={selectedProjectKey}
						busySessions={busySessions}
						mainChatActive={mainChatActive}
						mainChatCurrentSessionId={mainChatCurrentSessionId}
						mainChatNewSessionTrigger={mainChatNewSessionTrigger}
						mainChatSessionActivityTrigger={mainChatSessionActivityTrigger}
						expandedSessions={sidebarState.expandedSessions}
						toggleSessionExpanded={sidebarState.toggleSessionExpanded}
						expandedProjects={sidebarState.expandedProjects}
						toggleProjectExpanded={sidebarState.toggleProjectExpanded}
						pinnedSessions={sidebarState.pinnedSessions}
						togglePinSession={sidebarState.togglePinSession}
						pinnedProjects={sidebarState.pinnedProjects}
						togglePinProject={sidebarState.togglePinProject}
						projectSortBy={projectActions.projectSortBy}
						setProjectSortBy={projectActions.setProjectSortBy}
						projectSortAsc={projectActions.projectSortAsc}
						setProjectSortAsc={projectActions.setProjectSortAsc}
						selectedProjectLabel={sessionData.selectedProjectLabel}
						projectSummaries={sessionData.projectSummaries}
						projectDefaultAgents={projectDefaultAgents}
						availableAgents={availableAgents}
						onClose={() => sidebarState.setMobileMenuOpen(false)}
						onNewChat={handleNewChat}
						onNewProject={() => projectActions.setNewProjectDialogOpen(true)}
						onProjectClear={handleProjectClear}
						onSessionClick={handleSessionClick}
						onNewChatInProject={handleNewChatInProject}
						onPinSession={sidebarState.togglePinSession}
						onRenameSession={(id) =>
							sessionDialogs.handleRenameSession(id, chatHistory)
						}
						onDeleteSession={sessionDialogs.handleDeleteSession}
						onPinProject={sidebarState.togglePinProject}
						onRenameProject={sessionDialogs.handleRenameProject}
						onDeleteProject={sessionDialogs.handleDeleteProject}
						onMainChatSelect={handleMainChatSelect}
						onMainChatSessionSelect={handleMainChatSessionSelect}
						onMainChatNewSession={handleMainChatNewSession}
						onSearchResultClick={handleSearchResultClick}
						messageSearchExtraHits={messageSearchExtraHits}
						onToggleApp={handleMobileToggleClick}
						onToggleLocale={toggleLocale}
						onToggleTheme={toggleTheme}
						onProjectSelect={handleProjectSelect}
						onProjectDefaultAgentChange={handleProjectDefaultAgentChange}
					/>
				)}

				<aside
					className={cn(
						"fixed inset-y-0 left-0 flex-col transition-all duration-200 z-40 hidden md:flex border-r border-transparent dark:border-transparent",
						sidebarState.sidebarCollapsed
							? "w-[4.5rem] items-center"
							: "w-[16.25rem] items-center",
					)}
					style={{
						backgroundColor: sidebarBg,
						borderRightColor: isDark ? "transparent" : "var(--sidebar-border)",
					}}
					data-spotlight="sidebar"
				>
					<div
						className={cn(
							"h-20 w-full flex items-center px-4",
							sidebarState.sidebarCollapsed
								? "justify-center"
								: "justify-center relative",
						)}
					>
						{!sidebarState.sidebarCollapsed && (
							<img
								src={
									isDark
										? "/octo_logo_new_white.png"
										: "/octo_logo_new_black.png"
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
							onClick={() => sidebarState.setSidebarCollapsed((prev) => !prev)}
							className={cn(
								"text-muted-foreground hover:text-primary",
								!sidebarState.sidebarCollapsed && "absolute right-3",
							)}
						>
							{sidebarState.sidebarCollapsed ? (
								<PanelRightClose className="w-4 h-4" />
							) : (
								<PanelLeftClose className="w-4 h-4" />
							)}
						</Button>
					</div>

					{sidebarState.sidebarCollapsed && (
						<div className="w-full px-2">
							<div className="h-px w-full bg-primary/50" />
						</div>
					)}

					{!sidebarState.sidebarCollapsed && chatHistory.length > 0 && (
						<>
							<div className="w-full px-4">
								<div className="h-px w-full bg-primary/50" />
							</div>
							<div
								className="w-full px-1.5 mt-2 flex-1 min-h-0 flex flex-col overflow-x-hidden"
								data-spotlight="session-list"
							>
								<SidebarSessions
									locale={locale}
									chatHistory={chatHistory}
									sessionHierarchy={sessionData.sessionHierarchy}
									sessionsByProject={sessionData.sessionsByProject}
									filteredSessions={sessionData.filteredSessions}
									selectedChatSessionId={selectedChatSessionId}
									busySessions={busySessions}
									mainChatActive={mainChatActive}
									mainChatCurrentSessionId={mainChatCurrentSessionId}
									mainChatNewSessionTrigger={mainChatNewSessionTrigger}
									mainChatSessionActivityTrigger={
										mainChatSessionActivityTrigger
									}
									expandedSessions={sidebarState.expandedSessions}
									toggleSessionExpanded={sidebarState.toggleSessionExpanded}
									expandedProjects={sidebarState.expandedProjects}
									toggleProjectExpanded={sidebarState.toggleProjectExpanded}
									pinnedSessions={sidebarState.pinnedSessions}
									togglePinSession={sidebarState.togglePinSession}
									pinnedProjects={sidebarState.pinnedProjects}
									togglePinProject={sidebarState.togglePinProject}
									projectSortBy={projectActions.projectSortBy}
									setProjectSortBy={projectActions.setProjectSortBy}
									projectSortAsc={projectActions.projectSortAsc}
									setProjectSortAsc={projectActions.setProjectSortAsc}
									selectedProjectLabel={sessionData.selectedProjectLabel}
									onNewChat={handleNewChat}
									onNewProject={() =>
										projectActions.setNewProjectDialogOpen(true)
									}
									onProjectClear={handleProjectClear}
									onSessionClick={handleSessionClick}
									onNewChatInProject={handleNewChatInProject}
									onPinSession={sidebarState.togglePinSession}
									onRenameSession={(id) =>
										sessionDialogs.handleRenameSession(id, chatHistory)
									}
									onDeleteSession={sessionDialogs.handleDeleteSession}
									onPinProject={sidebarState.togglePinProject}
									onRenameProject={sessionDialogs.handleRenameProject}
									onDeleteProject={sessionDialogs.handleDeleteProject}
									onMainChatSelect={handleMainChatSelect}
									onMainChatSessionSelect={handleMainChatSessionSelect}
									onMainChatNewSession={handleMainChatNewSession}
									onSearchResultClick={handleSearchResultClick}
									messageSearchExtraHits={messageSearchExtraHits}
								/>
							</div>
						</>
					)}

					{sidebarState.sidebarCollapsed &&
						(chatHistory.length > 0 || opencodeSessions.length > 0) && (
							<div className="w-full px-2 mt-4">
								<div className="pt-2">
									<button
										type="button"
										onClick={() => sidebarState.setSidebarCollapsed(false)}
										className="w-full p-2 text-muted-foreground hover:text-foreground transition-colors"
										title={
											locale === "de" ? "Verlauf anzeigen" : "Show history"
										}
									>
										<Clock className="w-4 h-4 mx-auto" />
									</button>
								</div>
							</div>
						)}

					<SidebarNav
						activeAppId={activeAppId}
						sidebarCollapsed={sidebarState.sidebarCollapsed}
						isDark={isDark}
						onToggleApp={toggleApp}
						onToggleLocale={toggleLocale}
						onToggleTheme={toggleTheme}
					/>
				</aside>

				<div
					className="flex-1 flex flex-col min-h-0 overflow-hidden"
					style={{ backgroundColor: shellBg }}
				>
					<div
						className={cn(
							"flex-1 min-h-0 overflow-hidden pt-[calc(3.5rem+env(safe-area-inset-top))] md:pt-0 transition-all duration-200 flex flex-col",
							sidebarState.sidebarCollapsed
								? "md:pl-[4.5rem]"
								: "md:pl-[16.25rem]",
						)}
					>
						<div className="flex-1 min-h-0 w-full pb-0 md:pb-0">
							{ActiveComponent ? <ActiveComponent /> : <EmptyState />}
						</div>
						<div className="flex-shrink-0">
							<StatusBar />
						</div>
					</div>
				</div>

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

				<CommandPalette
					open={commandPaletteOpen}
					onOpenChange={setCommandPaletteOpen}
				/>

				<DeleteConfirmDialog
					open={sessionDialogs.deleteDialogOpen}
					onOpenChange={sessionDialogs.setDeleteDialogOpen}
					onConfirm={() =>
						sessionDialogs.handleConfirmDelete(
							deleteChatSession,
							chatHistory,
							opencodeBaseUrl,
							ensureOpencodeRunning,
						)
					}
					locale={locale}
				/>

				<RenameSessionDialog
					open={sessionDialogs.renameDialogOpen}
					onOpenChange={sessionDialogs.setRenameDialogOpen}
					initialValue={sessionDialogs.renameInitialValue}
					onConfirm={(newTitle) =>
						sessionDialogs.handleConfirmRename(newTitle, renameChatSession)
					}
					locale={locale}
				/>

				<DeleteConfirmDialog
					open={sessionDialogs.deleteProjectDialogOpen}
					onOpenChange={sessionDialogs.setDeleteProjectDialogOpen}
					onConfirm={() =>
						sessionDialogs.handleConfirmDeleteProject(
							chatHistory,
							deleteChatSession,
							opencodeBaseUrl,
							ensureOpencodeRunning,
						)
					}
					locale={locale}
					title={
						locale === "de"
							? `Projekt "${sessionDialogs.targetProjectName}" loschen?`
							: `Delete project "${sessionDialogs.targetProjectName}"?`
					}
					description={
						locale === "de"
							? "Diese Aktion kann nicht ruckgangig gemacht werden. Alle Chats in diesem Projekt werden dauerhaft geloscht."
							: "This action cannot be undone. All chats in this project will be permanently deleted."
					}
				/>

				<RenameProjectDialog
					open={sessionDialogs.renameProjectDialogOpen}
					onOpenChange={sessionDialogs.setRenameProjectDialogOpen}
					initialValue={sessionDialogs.renameProjectInitialValue}
					onConfirm={sessionDialogs.handleConfirmRenameProject}
					locale={locale}
				/>

				<NewProjectDialog
					open={projectActions.newProjectDialogOpen}
					onOpenChange={projectActions.handleNewProjectDialogChange}
					locale={locale}
					templatesLoading={projectActions.templatesLoading}
					templatesError={projectActions.templatesError}
					templatesConfigured={projectActions.templatesConfigured}
					projectTemplates={projectActions.projectTemplates}
					selectedTemplatePath={projectActions.selectedTemplatePath}
					onSelectTemplate={(path) =>
						projectActions.setSelectedTemplatePath(path)
					}
					newProjectPath={projectActions.newProjectPath}
					onProjectPathChange={projectActions.handleNewProjectPathChange}
					newProjectShared={projectActions.newProjectShared}
					onSharedChange={projectActions.setNewProjectShared}
					newProjectError={projectActions.newProjectError}
					newProjectSubmitting={projectActions.newProjectSubmitting}
					onSubmit={projectActions.handleCreateProjectFromTemplate}
				/>
			</div>
		</UIControlProvider>
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
