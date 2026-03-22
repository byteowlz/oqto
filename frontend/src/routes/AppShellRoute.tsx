import { AppProvider, useOnboarding } from "@/components/app-context";
import { CommandPalette } from "@/components/command-palette";
import { useChatContext } from "@/components/contexts";

import { StatusBar } from "@/components/status-bar";
import { Button } from "@/components/ui/button";
import { useApp } from "@/hooks/use-app";
import { useCurrentUser, useLogout } from "@/hooks/use-auth";
import { useCommandPalette } from "@/hooks/use-command-palette";
import { getUserDisplayName } from "@/lib/api/types";
import type { HstrySearchHit } from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import { Clock, PanelLeftClose, PanelRightClose } from "lucide-react";
import { useTheme } from "next-themes";
import { memo, useCallback, useDeferredValue, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";
import "@/apps";
import { UIControlProvider } from "@/components/contexts/ui-control-context";

import type { SearchMode } from "@/components/search";
import {
	type SharedWorkspaceInfo,
	convertToSharedWorkspace,
	createSharedWorkspace,
	createSharedWorkspaceWorkdir,
	deleteSharedWorkspace,
	updateSharedWorkspace,
} from "@/lib/api/shared-workspaces";
import { triggerChatHistoryBackfill } from "@/lib/api/chat";
import {
	DeleteConfirmDialog,
	MobileHeader,
	MobileMenu,
	NewProjectDialog,
	RenameProjectDialog,
	RenameSessionDialog,
	SidebarNav,
	SidebarSessions,
	SidebarSharedWorkspaces,
	useAppShellBootstrap,
	useAppShellProjectEvents,
	useAppShellRouteSync,
	useAppShellSessionAutomation,
	useAppShellSettings,
	useBranchGraphShortcut,
	useGodmodeShortcut,
	useProjectActions,
	useSessionData,
	useSessionDialogs,
	useShellLoadingState,
	useSidebarState,
} from "./app-shell";
import {
	ConvertToSharedDialog,
	SharedWorkspaceDialog,
	SharedWorkspaceMembersDialog,
} from "./app-shell/dialogs";
import { BranchGraphDialog } from "./app-shell/dialogs/BranchGraphDialog";
import { useSharedWorkspaces } from "./app-shell/hooks/useSharedWorkspaces";

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
		selectedChatSessionId,
		setSelectedChatSessionId,
		selectedChatFromHistory,
		selectedWorkspaceOverviewPath,
		setSelectedWorkspaceOverviewPath,
		createOptimisticChatSession,
		clearOptimisticChatSession,
		createNewChat,
		deleteChatSession,
		renameChatSession,
		busySessions,
		runnerSessions,
		runnerSessionCount,
		projectDefaultAgents,
		setProjectDefaultAgents,
		setScrollToMessageId,
		projects,
		refreshChatHistory,
		refreshWorkspaceSessions,
	} = useApp();

	const { t } = useTranslation();
	const { chatHistoryError } = useChatContext();

	const location = useLocation();
	const navigate = useNavigate();
	const { setTheme, resolvedTheme } = useTheme();
	const { activateGodmode, state: onboardingState } = useOnboarding();
	const { mounted, shellReady, barVisible, barWidth, barFade } =
		useShellLoadingState();
	const [selectedProjectKey, setSelectedProjectKey] = useState<string | null>(
		null,
	);
	const [sessionSearch, setSessionSearch] = useState("");
	const deferredSearch = useDeferredValue(sessionSearch);
	const [searchMode, setSearchMode] = useState<"sessions" | "messages">(
		"sessions",
	);
	const [branchGraphOpen, setBranchGraphOpen] = useState(false);

	const { mutate: handleLogout } = useLogout();
	const { data: currentUser } = useCurrentUser();
	const isAdmin = (currentUser?.role ?? "").toLowerCase() === "admin";

	// Use extracted hooks
	const sidebarState = useSidebarState();
	const projectActions = useProjectActions(
		selectedChatFromHistory?.workspace_path ?? null,
	);
	const sessionDialogs = useSessionDialogs();

	// Shared workspaces (must be before sessionData so we can filter)
	const sharedWs = useSharedWorkspaces();

	// Filter out sessions that belong to shared workspaces.
	// A session belongs to a shared workspace if its workspace_path starts
	// with the shared workspace's path, or if it has shared_workspace_id set.
	const personalChatHistory = useMemo(() => {
		const swPaths = sharedWs.sharedWorkspaces.map((ws) =>
			ws.path.replace(/\/$/, ""),
		);
		if (swPaths.length === 0) return chatHistory;
		return chatHistory.filter((session) => {
			if (session.shared_workspace_id) return false;
			const wp = session.workspace_path?.replace(/\/$/, "");
			if (!wp) return true;
			return !swPaths.some((swp) => wp === swp || wp.startsWith(`${swp}/`));
		});
	}, [chatHistory, sharedWs.sharedWorkspaces]);

	const sessionData = useSessionData({
		chatHistory: personalChatHistory,
		workspaceDirectories: projectActions.workspaceDirectories,
		locale,
		deferredSearch,
		pinnedSessions: sidebarState.pinnedSessions,
		pinnedProjects: sidebarState.pinnedProjects,
		selectedProjectKey,
		projectSortBy: projectActions.projectSortBy,
		projectSortAsc: projectActions.projectSortAsc,
	});
	const [swDialogOpen, setSwDialogOpen] = useState(false);
	const [swEditTarget, setSwEditTarget] = useState<SharedWorkspaceInfo | null>(
		null,
	);
	const [swMembersTarget, setSwMembersTarget] =
		useState<SharedWorkspaceInfo | null>(null);
	const [swSubmitting, setSwSubmitting] = useState(false);
	const [swError, setSwError] = useState<string | null>(null);

	const handleCreateOrEditSharedWorkspace = useCallback(
		async (data: {
			name: string;
			description: string;
			icon: string;
			color: string;
		}) => {
			try {
				setSwSubmitting(true);
				setSwError(null);
				if (swEditTarget) {
					await updateSharedWorkspace(swEditTarget.id, {
						name: data.name,
						description: data.description || undefined,
						icon: data.icon,
						color: data.color,
					});
				} else {
					await createSharedWorkspace({
						name: data.name,
						description: data.description || undefined,
						icon: data.icon,
						color: data.color,
					});
				}
				setSwDialogOpen(false);
				setSwEditTarget(null);
				await sharedWs.refresh();
			} catch (err) {
				setSwError(
					err instanceof Error ? err.message : "Failed to save workspace",
				);
			} finally {
				setSwSubmitting(false);
			}
		},
		[swEditTarget, sharedWs.refresh],
	);

	// Convert project to shared workspace
	const [convertDialogOpen, setConvertDialogOpen] = useState(false);
	const [convertSourcePath, setConvertSourcePath] = useState("");
	const [convertProjectName, setConvertProjectName] = useState("");
	const [convertSubmitting, setConvertSubmitting] = useState(false);
	const [convertError, setConvertError] = useState<string | null>(null);

	const handleShareProject = useCallback(
		(directory: string, projectName: string) => {
			setConvertSourcePath(directory);
			setConvertProjectName(projectName);
			setConvertError(null);
			setConvertDialogOpen(true);
		},
		[],
	);

	const handleConvertToShared = useCallback(
		async (data: {
			sourcePath: string;
			mode: "new" | "existing";
			workspaceName?: string;
			description?: string;
			icon?: string;
			color?: string;
			workspaceId?: string;
			workdirName?: string;
		}) => {
			try {
				setConvertSubmitting(true);
				setConvertError(null);
				if (data.mode === "existing") {
					if (!data.workspaceId) {
						throw new Error("Select a shared workspace.");
					}
					await createSharedWorkspaceWorkdir(data.workspaceId, {
						source_path: data.sourcePath,
						name: data.workdirName || undefined,
					});
				} else {
					await convertToSharedWorkspace({
						source_path: data.sourcePath,
						name: data.workspaceName || data.sourcePath,
						description: data.description || undefined,
						icon: data.icon,
						color: data.color,
					});
				}
				setConvertDialogOpen(false);
				await sharedWs.refresh();
			} catch (err) {
				setConvertError(err instanceof Error ? err.message : "Failed to share");
			} finally {
				setConvertSubmitting(false);
			}
		},
		[sharedWs.refresh],
	);

	const handleDeleteSharedWorkspace = useCallback(
		async (workspace: SharedWorkspaceInfo) => {
			if (
				!window.confirm(
					`Delete shared workspace "${workspace.name}"? This cannot be undone.`,
				)
			)
				return;
			try {
				await deleteSharedWorkspace(workspace.id);
				await sharedWs.refresh();
			} catch {
				// ignore
			}
		},
		[sharedWs.refresh],
	);

	const handleBulkDeleteSessions = useCallback(
		async (sessionIds: string[]) => {
			const failures: string[] = [];
			await Promise.all(
				sessionIds.map(async (sessionId) => {
					const ok = await deleteChatSession(sessionId);
					if (!ok) {
						failures.push(sessionId);
					}
				}),
			);
			return failures;
		},
		[deleteChatSession],
	);

	const handleDeleteSession = useCallback(
		async (sessionId: string) => deleteChatSession(sessionId),
		[deleteChatSession],
	);

	const handleBackfillProjectSessions = useCallback(
		async (workspacePath: string) => {
			try {
				const result = await triggerChatHistoryBackfill({ workspace: workspacePath });
				await refreshChatHistory({ force: true });
				await refreshWorkspaceSessions();
				return result;
			} catch (error) {
				console.error("Failed to backfill project sessions", error);
				throw error;
			}
		},
		[refreshChatHistory, refreshWorkspaceSessions],
	);

	useAppShellSessionAutomation({
		selectedChatSessionId,
		chatHistory,
		projectKeyForSession: sessionData.projectKeyForSession,
		expandedProjects: sidebarState.expandedProjects,
		toggleProjectExpanded: sidebarState.toggleProjectExpanded,
		workspaceDirectories: projectActions.workspaceDirectories,
		createNewChat,
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
	useAppShellRouteSync({
		activeAppId,
		setActiveAppId,
		matchedAppId,
		pathname: location.pathname,
		sessionsRoute,
		virtualApps,
		apps,
		navigate,
	});

	useBranchGraphShortcut(() => setBranchGraphOpen(true));

	const currentTheme = mounted ? resolvedTheme : "light";
	const isDark = currentTheme === "dark";
	const ActiveComponent = activeApp?.component ?? null;

	useGodmodeShortcut({
		activateGodmode,
		onboardingCompleted: onboardingState.completed,
		onboardingGodmode: onboardingState.godmode,
	});

	const _availableAgents = useMemo(() => [], []);

	// Auto-bootstrap: when a new user has no workspaces, automatically create
	// one using the username as display name. No dialog prompt needed -- the
	// agent can rename it during bootstrap if configured to do so.
	const currentDisplayName = getUserDisplayName(currentUser);
	useAppShellBootstrap({
		projectsLength: projects.length,
		chatHistoryLength: chatHistory.length,
		currentDisplayName,
		locale,
		refreshChatHistory,
		refreshWorkspaceSessions,
		refreshWorkspaceDirectories: projectActions.refreshWorkspaceDirectories,
		setSelectedWorkspaceOverviewPath,
		setSelectedChatSessionId,
		setActiveAppId,
	});

	useAppShellSettings();

	const messageSearchExtraHits = useMemo(
		() => sessionData.sessionTitleHits,
		[sessionData.sessionTitleHits],
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

	const handleProjectOverview = useCallback(
		(directory: string) => {
			setSelectedChatSessionId(null);
			setSelectedWorkspaceOverviewPath(directory);
			setActiveAppId("sessions");
			if (sessionsRoute) navigate(sessionsRoute);
			sidebarState.setMobileMenuOpen(false);
		},
		[
			navigate,
			sessionsRoute,
			setActiveAppId,
			setSelectedChatSessionId,
			setSelectedWorkspaceOverviewPath,
			sidebarState,
		],
	);

	const handleProjectClear = useCallback(() => {
		setSelectedProjectKey(null);
	}, []);

	// biome-ignore lint/correctness/useExhaustiveDependencies: setSelectedProjectKey is stable setState
	const handleSessionClick = useCallback(
		(sessionId: string) => {
			setSelectedWorkspaceOverviewPath(null);
			setSelectedChatSessionId(sessionId);
			setSelectedProjectKey(null);
			setActiveAppId("sessions");
			if (sessionsRoute) navigate(sessionsRoute);
			sidebarState.setMobileMenuOpen(false);
		},
		[
			navigate,
			sessionsRoute,
			setActiveAppId,
			setSelectedChatSessionId,
			setSelectedWorkspaceOverviewPath,
			setSelectedProjectKey,
			sidebarState,
		],
	);

	// biome-ignore lint/correctness/useExhaustiveDependencies: setSelectedProjectKey is stable setState
	const handleSearchResultClick = useCallback(
		(hit: HstrySearchHit) => {
			setSessionSearch("");
			const targetMessageId =
				hit.message_id || (hit.line_number ? `line-${hit.line_number}` : null);
			if (targetMessageId) setScrollToMessageId(targetMessageId);

			if (hit.agent === "pi_agent") {
				const sessionId = hit.session_id || "";
				if (sessionId) {
					setSelectedChatSessionId(sessionId);
				}
				setSelectedWorkspaceOverviewPath(null);
				setSelectedProjectKey(null);
				setActiveAppId("sessions");
				if (sessionsRoute) navigate(sessionsRoute);
			}
			sidebarState.setMobileMenuOpen(false);
		},
		[
			setActiveAppId,
			setSelectedChatSessionId,
			setSelectedWorkspaceOverviewPath,
			setSelectedProjectKey,
			setScrollToMessageId,
			navigate,
			sessionsRoute,
			sidebarState,
		],
	);

	const handleNewChat = useCallback(async () => {
		setSelectedWorkspaceOverviewPath(null);
		if (selectedProjectKey) {
			const project = sessionData.projectSummaries.find(
				(p) => p.key === selectedProjectKey,
			);
			if (project?.directory) {
				setActiveAppId("sessions");
				await createNewChat(project.directory);
				return;
			}
		}

		const currentWorkspacePath = selectedChatFromHistory?.workspace_path;

		setActiveAppId("sessions");
		await createNewChat(currentWorkspacePath ?? undefined);
	}, [
		selectedChatFromHistory,
		selectedProjectKey,
		sessionData.projectSummaries,
		createNewChat,
		setActiveAppId,
		setSelectedWorkspaceOverviewPath,
	]);

	const handleNewChatInProject = useCallback(
		async (directory: string) => {
			setSelectedWorkspaceOverviewPath(null);
			setActiveAppId("sessions");
			sidebarState.setMobileMenuOpen(false);
			await createNewChat(directory);
		},
		[
			createNewChat,
			setActiveAppId,
			setSelectedWorkspaceOverviewPath,
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

	useAppShellProjectEvents({
		setSelectedProjectKey,
		setActiveAppId,
		onProjectDefaultAgentChange: handleProjectDefaultAgentChange,
	});

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
					onMenuOpen={() => sidebarState.setMobileMenuOpen(true)}
					onNewChat={handleNewChat}
				/>

				{sidebarState.mobileMenuOpen && (
					<MobileMenu
						locale={locale}
						isDark={isDark}
						activeAppId={activeAppId}
						isAdmin={isAdmin}
						chatHistory={chatHistory}
						sessionHierarchy={sessionData.sessionHierarchy}
						sessionsByProject={sessionData.sessionsByProject}
						filteredSessions={sessionData.filteredSessions}
						selectedChatSessionId={selectedChatSessionId}
						selectedProjectKey={selectedProjectKey}
						busySessions={busySessions}
						runnerSessionCount={runnerSessionCount}
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
						availableAgents={_availableAgents}
						onClose={() => sidebarState.setMobileMenuOpen(false)}
						onNewChat={handleNewChat}
						onNewProject={() => projectActions.setNewProjectDialogOpen(true)}
						onProjectClear={handleProjectClear}
						onProjectOverview={handleProjectOverview}
						onSessionClick={handleSessionClick}
						onNewChatInProject={handleNewChatInProject}
						onPinSession={sidebarState.togglePinSession}
						onRenameSession={(id) =>
							sessionDialogs.handleRenameSession(id, chatHistory)
						}
						onDeleteSession={handleDeleteSession}
						onBulkDeleteSessions={handleBulkDeleteSessions}
						onPinProject={sidebarState.togglePinProject}
						onRenameProject={sessionDialogs.handleRenameProject}
						onDeleteProject={sessionDialogs.handleDeleteProject}
						onBackfillProject={(directory) => {
							void handleBackfillProjectSessions(directory).catch(() => {});
						}}
						onSearchResultClick={handleSearchResultClick}
						messageSearchExtraHits={messageSearchExtraHits}
						sessionSearch={sessionSearch}
						onSessionSearchChange={setSessionSearch}
						searchMode={searchMode}
						onSearchModeChange={setSearchMode}
						onToggleApp={handleMobileToggleClick}
						onToggleLocale={toggleLocale}
						onToggleTheme={toggleTheme}
						onLogout={handleLogout}
						onProjectSelect={handleProjectSelect}
						onProjectDefaultAgentChange={handleProjectDefaultAgentChange}
						sharedWorkspaces={sharedWs.sharedWorkspaces}
						expandedWorkspaces={sharedWs.expandedWorkspaces}
						toggleWorkspaceExpanded={sharedWs.toggleWorkspaceExpanded}
						onNewSharedWorkspace={() => {
							setSwEditTarget(null);
							setSwError(null);
							setSwDialogOpen(true);
						}}
						onManageWorkspace={(ws) => {
							setSwEditTarget(ws);
							setSwError(null);
							setSwDialogOpen(true);
						}}
						onManageMembers={(ws) => setSwMembersTarget(ws)}
						onNewChatInWorkspace={(ws) => {
							void createNewChat(ws.path, ws.id);
							sidebarState.setMobileMenuOpen(false);
						}}
						onNewProjectInWorkspace={(ws) => {
							projectActions.openNewProjectForWorkspace(ws.path, ws.id);
							sidebarState.setMobileMenuOpen(false);
						}}
						onDeleteWorkspace={handleDeleteSharedWorkspace}
						onSelectWorkdir={(ws, wd) => {
							void createNewChat(wd.path, ws.id);
							sidebarState.setMobileMenuOpen(false);
						}}
						runnerSessions={runnerSessions}
						onSharedSessionClick={(session, sharedWorkspaceId) => {
							createOptimisticChatSession(
								session.id,
								session.workspace_path ?? undefined,
								sharedWorkspaceId,
								session,
							);
							setSelectedChatSessionId(session.id);
							sidebarState.setMobileMenuOpen(false);
						}}
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
								src={isDark ? "/oqto_logo_white.svg" : "/oqto_logo_black.svg"}
								alt="OQTO"
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

					{!sidebarState.sidebarCollapsed && chatHistoryError && (
						<div className="w-full px-3 mt-2">
							<div className="bg-destructive/15 border border-destructive/30 rounded-md p-3 text-xs">
								<div className="font-medium text-destructive mb-1">
									{t("chat.chatHistoryUnavailable")}
								</div>
								<div className="text-muted-foreground mb-2 break-words">
									{chatHistoryError}
								</div>
								<button
									type="button"
									onClick={() => refreshChatHistory()}
									className="text-xs text-primary hover:underline"
								>
									{t("chat.retry")}
								</button>
							</div>
						</div>
					)}

					{!sidebarState.sidebarCollapsed &&
						(chatHistory.length > 0 ||
							projectActions.workspaceDirectories.length > 0 ||
							sharedWs.sharedWorkspaces.length > 0) && (
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
										chatHistory={personalChatHistory}
										sessionHierarchy={sessionData.sessionHierarchy}
										sessionsByProject={sessionData.sessionsByProject}
										filteredSessions={sessionData.filteredSessions}
										selectedChatSessionId={selectedChatSessionId}
										busySessions={busySessions}
										runnerSessionCount={runnerSessionCount}
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
										onProjectOverview={handleProjectOverview}
										onSessionClick={handleSessionClick}
										onNewChatInProject={handleNewChatInProject}
										onPinSession={sidebarState.togglePinSession}
										onRenameSession={(id) =>
											sessionDialogs.handleRenameSession(id, chatHistory)
										}
										onDeleteSession={handleDeleteSession}
										onBulkDeleteSessions={handleBulkDeleteSessions}
										onPinProject={sidebarState.togglePinProject}
										onRenameProject={sessionDialogs.handleRenameProject}
										onDeleteProject={sessionDialogs.handleDeleteProject}
										onBackfillProject={(directory) => {
											void handleBackfillProjectSessions(directory).catch(() => {});
										}}
										onShareProject={handleShareProject}
										onSearchResultClick={handleSearchResultClick}
										messageSearchExtraHits={messageSearchExtraHits}
										sessionSearch={sessionSearch}
										onSessionSearchChange={setSessionSearch}
										searchMode={searchMode}
										onSearchModeChange={setSearchMode}
										belowSearchSlot={
											sharedWs.sharedWorkspaces.length > 0 ? (
												<>
													<SidebarSharedWorkspaces
														sharedWorkspaces={sharedWs.sharedWorkspaces}
														expandedWorkspaces={sharedWs.expandedWorkspaces}
														toggleWorkspaceExpanded={
															sharedWs.toggleWorkspaceExpanded
														}
														onNewSharedWorkspace={() => {
															setSwEditTarget(null);
															setSwError(null);
															setSwDialogOpen(true);
														}}
														onManageWorkspace={(ws) => {
															setSwEditTarget(ws);
															setSwError(null);
															setSwDialogOpen(true);
														}}
														onManageMembers={(ws) => setSwMembersTarget(ws)}
														onNewChatInWorkspace={(ws) => {
															void createNewChat(ws.path, ws.id);
														}}
														onNewProjectInWorkspace={(ws) => {
															projectActions.openNewProjectForWorkspace(
																ws.path,
																ws.id,
															);
														}}
														onDeleteWorkspace={handleDeleteSharedWorkspace}
														onSelectWorkdir={(ws, wd) => {
															void createNewChat(wd.path, ws.id);
														}}
														chatHistory={chatHistory}
														runnerSessions={runnerSessions}
														busySessions={busySessions}
														selectedChatSessionId={selectedChatSessionId}
														onSessionClick={(session, sharedWorkspaceId) => {
															createOptimisticChatSession(
																session.id,
																session.workspace_path ?? undefined,
																sharedWorkspaceId,
																session,
															);
															setSelectedChatSessionId(session.id);
														}}
														onRenameSession={(id) =>
															sessionDialogs.handleRenameSession(
																id,
																chatHistory,
															)
														}
														onDeleteSession={handleDeleteSession}
														onPinSession={sidebarState.togglePinSession}
														pinnedSessions={sidebarState.pinnedSessions}
														onPinProject={sidebarState.togglePinProject}
														onRenameProject={sessionDialogs.handleRenameProject}
														onDeleteProject={sessionDialogs.handleDeleteProject}
														pinnedProjects={sidebarState.pinnedProjects}
													/>
													<div className="w-full px-2 my-1">
														<div className="h-px w-full bg-sidebar-border/50" />
													</div>
												</>
											) : undefined
										}
									/>
								</div>
							</>
						)}

					{sidebarState.sidebarCollapsed && chatHistory.length > 0 && (
						<div className="w-full px-2 mt-4">
							<div className="pt-2">
								<button
									type="button"
									onClick={() => sidebarState.setSidebarCollapsed(false)}
									className="w-full p-2 text-muted-foreground hover:text-foreground transition-colors"
									title={t("sessions.showHistory")}
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
						isAdmin={isAdmin}
						username={currentDisplayName}
						onToggleApp={toggleApp}
						onToggleLocale={toggleLocale}
						onToggleTheme={toggleTheme}
						onLogout={handleLogout}
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

				<BranchGraphDialog
					open={branchGraphOpen}
					onOpenChange={setBranchGraphOpen}
					parentSessions={sessionData.sessionHierarchy.parentSessions}
					childSessionsByParent={
						sessionData.sessionHierarchy.childSessionsByParent
					}
					selectedSessionId={selectedChatSessionId}
					onSelectSession={setSelectedChatSessionId}
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
						)
					}
					locale={locale}
					title={t("projects.deleteProjectTitle", {
						name: sessionDialogs.targetProjectName,
					})}
					description={t("projects.deleteProjectDescription")}
				/>

				<RenameProjectDialog
					open={sessionDialogs.renameProjectDialogOpen}
					onOpenChange={sessionDialogs.setRenameProjectDialogOpen}
					initialValue={sessionDialogs.renameProjectInitialValue}
					onConfirm={(newName) =>
						sessionDialogs.handleConfirmRenameProject(
							newName,
							refreshChatHistory,
							projectActions.refreshWorkspaceDirectories,
						)
					}
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
					isSharedWorkspaceContext={
						!!projectActions.newProjectSharedWorkspaceId
					}
					newProjectError={projectActions.newProjectError}
					newProjectSubmitting={projectActions.newProjectSubmitting}
					newProjectSettings={projectActions.newProjectSettings}
					onProjectSettingsChange={projectActions.setNewProjectSettings}
					availableModels={projectActions.availableModels}
					availableSkills={projectActions.availableSkills}
					availableExtensions={projectActions.availableExtensions}
					sandboxProfiles={projectActions.sandboxProfiles}
					settingsLoading={projectActions.settingsLoading}
					onSubmit={projectActions.handleCreateProjectFromTemplate}
				/>

				{/* Shared workspace dialogs */}
				<SharedWorkspaceDialog
					open={swDialogOpen}
					onOpenChange={(open) => {
						setSwDialogOpen(open);
						if (!open) setSwEditTarget(null);
					}}
					editId={swEditTarget?.id}
					initialName={swEditTarget?.name ?? ""}
					initialDescription={swEditTarget?.description ?? ""}
					initialIcon={swEditTarget?.icon ?? "users"}
					initialColor={swEditTarget?.color ?? "#3ba77c"}
					submitting={swSubmitting}
					error={swError}
					onSubmit={handleCreateOrEditSharedWorkspace}
				/>

				{swMembersTarget && (
					<SharedWorkspaceMembersDialog
						open={!!swMembersTarget}
						onOpenChange={(open) => {
							if (!open) setSwMembersTarget(null);
						}}
						workspaceId={swMembersTarget.id}
						workspaceName={swMembersTarget.name}
						workspaceColor={swMembersTarget.color}
						myRole={swMembersTarget.my_role}
					/>
				)}

				<ConvertToSharedDialog
					open={convertDialogOpen}
					onOpenChange={setConvertDialogOpen}
					sourcePath={convertSourcePath}
					sourceProjectName={convertProjectName}
					sharedWorkspaces={sharedWs.sharedWorkspaces}
					submitting={convertSubmitting}
					error={convertError}
					onSubmit={handleConvertToShared}
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
