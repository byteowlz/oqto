import { type SearchMode } from "@/components/search";
import { Button } from "@/components/ui/button";
import type { ChatSession, HstrySearchHit } from "@/lib/control-plane-client";
import type { OpenCodeAgent } from "@/lib/opencode-client";
import { formatSessionDate } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	Bot,
	FolderKanban,
	Globe2,
	LayoutDashboard,
	LogOut,
	MoonStar,
	Settings,
	Shield,
	SunMedium,
	X,
} from "lucide-react";
import { memo } from "react";
import {
	type SessionHierarchy,
	type SessionsByProject,
	SidebarSessions,
} from "./SidebarSessions";

const sidebarBg = "var(--sidebar, #181b1a)";

export interface ProjectSummary {
	key: string;
	name: string;
	directory?: string;
	sessionCount: number;
	lastActive: number;
}

export interface MobileMenuProps {
	locale: string;
	isDark: boolean;
	activeAppId: string;
	chatHistory: ChatSession[];
	sessionHierarchy: SessionHierarchy;
	sessionsByProject: SessionsByProject[];
	filteredSessions: ChatSession[];
	selectedChatSessionId: string | null;
	selectedProjectKey: string | null;
	busySessions: Set<string>;
	runnerSessionCount: number;
	expandedSessions: Set<string>;
	toggleSessionExpanded: (sessionId: string) => void;
	expandedProjects: Set<string>;
	toggleProjectExpanded: (projectKey: string) => void;
	pinnedSessions: Set<string>;
	togglePinSession: (sessionId: string) => void;
	pinnedProjects: string[];
	togglePinProject: (projectKey: string) => void;
	projectSortBy: "date" | "name" | "sessions";
	setProjectSortBy: (sort: "date" | "name" | "sessions") => void;
	projectSortAsc: boolean;
	setProjectSortAsc: (asc: boolean) => void;
	selectedProjectLabel: string | null;
	projectSummaries: ProjectSummary[];
	projectDefaultAgents: Record<string, string>;
	availableAgents: OpenCodeAgent[];
	onClose: () => void;
	onNewChat: () => void;
	onNewProject: () => void;
	onProjectClear: () => void;
	onProjectOverview: (directory: string) => void;
	onSessionClick: (sessionId: string) => void;
	onNewChatInProject: (directory: string) => void;
	onPinSession: (sessionId: string) => void;
	onRenameSession: (sessionId: string) => void;
	onDeleteSession: (sessionId: string) => void;
	onBulkDeleteSessions: (sessionIds: string[]) => Promise<string[] | undefined>;
	onPinProject: (projectKey: string) => void;
	onRenameProject: (projectKey: string, currentName: string) => void;
	onDeleteProject: (projectKey: string, projectName: string) => void;
	onSearchResultClick: (hit: HstrySearchHit) => void;
	messageSearchExtraHits: HstrySearchHit[];
	isAdmin?: boolean;
	onToggleApp: (appId: string) => void;
	onToggleLocale: () => void;
	onToggleTheme: () => void;
	onLogout: () => void;
	onProjectSelect: (projectKey: string) => void;
	onProjectDefaultAgentChange: (projectKey: string, agentId: string) => void;
	// Search props
	sessionSearch?: string;
	onSessionSearchChange?: (query: string) => void;
	searchMode?: SearchMode;
	onSearchModeChange?: (mode: SearchMode) => void;
}

export const MobileMenu = memo(function MobileMenu({
	locale,
	isDark,
	activeAppId,
	chatHistory,
	sessionHierarchy,
	sessionsByProject,
	filteredSessions,
	selectedChatSessionId,
	selectedProjectKey,
	busySessions,
	runnerSessionCount,
	expandedSessions,
	toggleSessionExpanded,
	expandedProjects,
	toggleProjectExpanded,
	pinnedSessions,
	togglePinSession,
	pinnedProjects,
	togglePinProject,
	projectSortBy,
	setProjectSortBy,
	projectSortAsc,
	setProjectSortAsc,
	selectedProjectLabel,
	projectSummaries,
	projectDefaultAgents,
	availableAgents,
	onClose,
	onNewChat,
	onNewProject,
	onProjectClear,
	onProjectOverview,
	onSessionClick,
	onNewChatInProject,
	onPinSession,
	onRenameSession,
	onDeleteSession,
	onBulkDeleteSessions,
	onPinProject,
	onRenameProject,
	onDeleteProject,
	onSearchResultClick,
	messageSearchExtraHits,
	isAdmin,
	onToggleApp,
	onToggleLocale,
	onToggleTheme,
	onLogout,
	onProjectSelect,
	onProjectDefaultAgentChange,
	sessionSearch,
	onSessionSearchChange,
	searchMode,
	onSearchModeChange,
}: MobileMenuProps) {
	return (
		<div
			className="fixed inset-0 z-50 flex flex-col md:hidden"
			style={{
				backgroundColor: sidebarBg,
				paddingTop: "env(safe-area-inset-top)",
			}}
		>
			<div className="h-14 flex items-center justify-between px-3">
				<img
					src={isDark ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"}
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
					onClick={onClose}
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
					<SidebarSessions
						locale={locale}
						chatHistory={chatHistory}
						sessionHierarchy={sessionHierarchy}
						sessionsByProject={sessionsByProject}
						filteredSessions={filteredSessions}
						selectedChatSessionId={selectedChatSessionId}
						busySessions={busySessions}
						runnerSessionCount={runnerSessionCount}
						expandedSessions={expandedSessions}
						toggleSessionExpanded={toggleSessionExpanded}
						expandedProjects={expandedProjects}
						toggleProjectExpanded={toggleProjectExpanded}
						pinnedSessions={pinnedSessions}
						togglePinSession={togglePinSession}
						pinnedProjects={pinnedProjects}
						togglePinProject={togglePinProject}
						projectSortBy={projectSortBy}
						setProjectSortBy={setProjectSortBy}
						projectSortAsc={projectSortAsc}
						setProjectSortAsc={setProjectSortAsc}
						selectedProjectLabel={selectedProjectLabel}
						onNewChat={onNewChat}
						onNewProject={onNewProject}
						onProjectClear={onProjectClear}
						onProjectOverview={onProjectOverview}
						onSessionClick={onSessionClick}
						onNewChatInProject={onNewChatInProject}
						onPinSession={onPinSession}
						onRenameSession={onRenameSession}
						onDeleteSession={onDeleteSession}
						onBulkDeleteSessions={onBulkDeleteSessions}
						onPinProject={onPinProject}
						onRenameProject={onRenameProject}
						onDeleteProject={onDeleteProject}
						onSearchResultClick={onSearchResultClick}
						messageSearchExtraHits={messageSearchExtraHits}
						sessionSearch={sessionSearch}
						onSessionSearchChange={onSessionSearchChange}
						searchMode={searchMode}
						onSearchModeChange={onSearchModeChange}
						isMobile
					/>
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
												onClick={() => onProjectSelect(project.key)}
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
														onProjectDefaultAgentChange(
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
								onClick={onClose}
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
						onClick={() => onToggleApp("dashboard")}
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
						onClick={() => onToggleApp("settings")}
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
					{isAdmin && (
						<Button
							type="button"
							variant="ghost"
							size="icon"
							rounded="full"
							onClick={() => onToggleApp("admin")}
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
					)}
					<Button
						type="button"
						variant="ghost"
						size="icon"
						rounded="full"
						onClick={onToggleLocale}
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
						onClick={onToggleTheme}
						aria-pressed={isDark}
						className="text-muted-foreground hover:text-primary hover:bg-sidebar-accent"
					>
						{isDark ? (
							<SunMedium className="w-5 h-5" />
						) : (
							<MoonStar className="w-5 h-5" />
						)}
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="icon"
						rounded="full"
						onClick={onLogout}
						aria-label="Logout"
						className="text-muted-foreground hover:text-primary hover:bg-sidebar-accent"
					>
						<LogOut className="w-5 h-5" />
					</Button>
				</div>
			</div>
		</div>
	);
});
