/**
 * Sidebar section displaying shared workspaces the user belongs to.
 * Each workspace shows its icon in configured color, name, member count,
 * and role badge. Expanding a workspace shows its workdirs (project directories).
 */
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import type {
	SharedWorkspaceInfo,
	SharedWorkspaceWorkdir,
} from "@/lib/api/shared-workspaces";
import { listWorkdirs } from "@/lib/api/shared-workspaces";
import { listChatHistory } from "@/lib/api/chat";
import type { ChatSession } from "@/lib/api/chat";
import { cn } from "@/lib/utils";
import {
	ChevronDown,
	ChevronRight,
	Folder,
	FolderPlus,
	MessageSquarePlus,
	Pencil,
	Plus,
	Settings,
	Trash2,
	UserPlus,
	Users2,
} from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { WorkspaceIcon } from "./WorkspaceIcon";

export interface SidebarSharedWorkspacesProps {
	sharedWorkspaces: SharedWorkspaceInfo[];
	expandedWorkspaces: Set<string>;
	toggleWorkspaceExpanded: (workspaceId: string) => void;
	onNewSharedWorkspace: () => void;
	onManageWorkspace: (workspace: SharedWorkspaceInfo) => void;
	onManageMembers: (workspace: SharedWorkspaceInfo) => void;
	onNewChatInWorkspace: (workspace: SharedWorkspaceInfo) => void;
	onNewProjectInWorkspace?: (workspace: SharedWorkspaceInfo) => void;
	onDeleteWorkspace: (workspace: SharedWorkspaceInfo) => void;
	/** Called when user clicks a workdir to start a new chat in it */
	onSelectWorkdir?: (workspace: SharedWorkspaceInfo, workdir: SharedWorkspaceWorkdir) => void;
	/** Runner sessions for all shared workspaces (filtered by shared_workspace_id) */
	runnerSessions?: Array<{
		session_id: string;
		state: string;
		cwd: string;
		last_activity: number;
		shared_workspace_id?: string;
	}>;
	/** Set of busy (working) session IDs */
	busySessions?: Set<string>;
	/** Called when user clicks a session in a shared workspace */
	onSessionClick?: (session: ChatSession) => void;
	isMobile?: boolean;
}

function RoleBadge({ role, color }: { role: string; color: string }) {
	return (
		<span
			className="text-[9px] uppercase tracking-wider px-1 py-0.5 font-medium"
			style={{
				color,
				backgroundColor: `${color}20`,
				border: `1px solid ${color}30`,
			}}
		>
			{role}
		</span>
	);
}

/** Fetches and displays workdirs for an expanded workspace, with sessions grouped under each */
function WorkdirList({
	workspace,
	isMobile,
	onSelectWorkdir,
	activeSessions,
	busySessions,
	onSessionClick,
}: {
	workspace: SharedWorkspaceInfo;
	isMobile: boolean;
	onSelectWorkdir?: (workspace: SharedWorkspaceInfo, workdir: SharedWorkspaceWorkdir) => void;
	activeSessions?: Array<{
		session_id: string;
		state: string;
		cwd: string;
		last_activity: number;
	}>;
	busySessions?: Set<string>;
	onSessionClick?: (session: ChatSession) => void;
}) {
	const [workdirs, setWorkdirs] = useState<SharedWorkspaceWorkdir[]>([]);
	const [hstrySessions, setHstrySessions] = useState<ChatSession[]>([]);
	const [loading, setLoading] = useState(true);

	useEffect(() => {
		let cancelled = false;
		setLoading(true);

		// Fetch workdirs and session history in parallel
		Promise.all([
			listWorkdirs(workspace.id),
			listChatHistory({ shared_workspace_id: workspace.id }).catch(() => [] as ChatSession[]),
		])
			.then(([wdData, sessionData]) => {
				if (!cancelled) {
					setWorkdirs(wdData);
					setHstrySessions(sessionData);
				}
			})
			.catch(() => {})
			.finally(() => {
				if (!cancelled) setLoading(false);
			});

		return () => {
			cancelled = true;
		};
	}, [workspace.id]);

	const textSize = isMobile ? "text-xs" : "text-[11px]";
	const smallText = isMobile ? "text-[10px]" : "text-[9px]";

	if (loading) {
		return (
			<div className={cn("px-5 py-1 text-muted-foreground/60", isMobile ? "text-xs" : "text-[10px]")}>
				...
			</div>
		);
	}

	if (workdirs.length === 0) {
		return (
			<div className={cn("px-5 py-1 text-muted-foreground/60 italic", isMobile ? "text-xs" : "text-[10px]")}>
				No projects yet
			</div>
		);
	}

	// Merge hstry sessions with active runner sessions (prefer runner data for active ones)
	const activeSessionIds = new Set((activeSessions ?? []).map((s) => s.session_id));

	// Group hstry sessions by workdir path
	const sessionsByWorkdir = new Map<string, ChatSession[]>();
	for (const s of hstrySessions) {
		const wp = s.workspace_path?.replace(/\/$/, "");
		for (const wd of workdirs) {
			const normalizedWdPath = wd.path.replace(/\/$/, "");
			if (wp === normalizedWdPath || wp?.startsWith(`${normalizedWdPath}/`)) {
				const existing = sessionsByWorkdir.get(wd.path) ?? [];
				existing.push(s);
				sessionsByWorkdir.set(wd.path, existing);
				break;
			}
		}
	}

	return (
		<div className="space-y-0.5">
			{workdirs.map((wd) => {
				const wdSessions = sessionsByWorkdir.get(wd.path) ?? [];
				const sessionCount = wdSessions.length;
				return (
					<div key={wd.path}>
						<button
							type="button"
							onClick={() => onSelectWorkdir?.(workspace, wd)}
							className={cn(
								"w-full flex items-center gap-1.5 px-5 py-1 text-left hover:bg-sidebar-accent/50 rounded transition-colors",
								textSize,
							)}
						>
							<Folder
								className={cn(
									"flex-shrink-0 text-muted-foreground",
									isMobile ? "w-3.5 h-3.5" : "w-3 h-3",
								)}
							/>
							<span className="text-foreground truncate">{wd.name}</span>
							{sessionCount > 0 && (
								<span className={cn("text-muted-foreground/50 ml-auto", smallText)}>
									{sessionCount}
								</span>
							)}
						</button>
						{/* Sessions under this workdir */}
						{wdSessions.map((s) => {
							const isBusy = activeSessionIds.has(s.id) && busySessions?.has(s.id);
							const isActive = activeSessionIds.has(s.id);
							return (
								<button
									key={s.id}
									type="button"
									onClick={() => onSessionClick?.(s)}
									className={cn(
										"w-full flex items-center gap-1.5 px-8 py-0.5 text-left hover:bg-sidebar-accent/50 rounded transition-colors",
										smallText,
									)}
								>
									<MessageSquarePlus
										className={cn(
											"flex-shrink-0",
											isBusy ? "text-green-500" : isActive ? "text-foreground/70" : "text-muted-foreground/50",
											isMobile ? "w-3 h-3" : "w-2.5 h-2.5",
										)}
									/>
									<span className="text-muted-foreground truncate">
										{s.title || s.id.slice(0, 12)}
									</span>
									{isBusy && (
										<span className="ml-auto w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse" />
									)}
								</button>
							);
						})}
					</div>
				);
			})}
		</div>
	);
}

export const SidebarSharedWorkspaces = memo(function SidebarSharedWorkspaces({
	sharedWorkspaces,
	expandedWorkspaces,
	toggleWorkspaceExpanded,
	onNewSharedWorkspace,
	onManageWorkspace,
	onManageMembers,
	onNewChatInWorkspace,
	onNewProjectInWorkspace,
	onDeleteWorkspace,
	onSelectWorkdir,
	runnerSessions,
	busySessions,
	onSessionClick,
	isMobile = false,
}: SidebarSharedWorkspacesProps) {
	const { t } = useTranslation();

	if (sharedWorkspaces.length === 0) {
		return null;
	}

	const sizeClasses = isMobile
		? {
				headerText: "text-xs",
				iconSize: "w-4 h-4",
				workspaceIcon: "w-4 h-4",
				text: "text-sm",
				buttonSize: "p-1.5",
				countText: "text-xs",
			}
		: {
				headerText: "text-xs",
				iconSize: "w-3 h-3",
				workspaceIcon: "w-3.5 h-3.5",
				text: "text-xs",
				buttonSize: "p-1",
				countText: "text-[10px]",
			};

	return (
		<div className="px-1">
			{/* Section header */}
			<div className="flex items-center justify-between gap-2 py-1.5 px-1">
				<div className="flex items-center gap-2">
					<Users2
						className={cn("text-muted-foreground", sizeClasses.iconSize)}
					/>
					<span
						className={cn(
							"uppercase tracking-wide text-muted-foreground",
							sizeClasses.headerText,
						)}
					>
						{t("sharedWorkspaces.title", "Shared")}
					</span>
					<span className={cn("text-muted-foreground/50", sizeClasses.countText)}>
						({sharedWorkspaces.length})
					</span>
				</div>
				<button
					type="button"
					onClick={onNewSharedWorkspace}
					className={cn(
						"text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded",
						sizeClasses.buttonSize,
					)}
					title={t("sharedWorkspaces.create", "New shared workspace")}
				>
					<Plus className={sizeClasses.iconSize} />
				</button>
			</div>

			{/* Workspace list */}
			<div className="space-y-0.5">
				{sharedWorkspaces.map((workspace) => {
					const isExpanded = expandedWorkspaces.has(workspace.id);
					const canManage =
						workspace.my_role === "owner" || workspace.my_role === "admin";

					return (
						<div
							key={workspace.id}
							className="border-b border-sidebar-border/50 last:border-b-0"
						>
							<ContextMenu>
								<ContextMenuTrigger className="contents">
									<div className="flex items-center gap-1 px-1 py-1.5 group">
										<button
											type="button"
											onClick={() => toggleWorkspaceExpanded(workspace.id)}
											className="flex items-center gap-1.5 text-left hover:bg-sidebar-accent/50 px-1 py-0.5 -mx-1"
										>
											{isExpanded ? (
												<ChevronDown
													className={cn(
														"text-muted-foreground flex-shrink-0",
														sizeClasses.iconSize,
													)}
												/>
											) : (
												<ChevronRight
													className={cn(
														"text-muted-foreground flex-shrink-0",
														sizeClasses.iconSize,
													)}
												/>
											)}
										</button>
										<button
											type="button"
											onClick={() => toggleWorkspaceExpanded(workspace.id)}
											className="flex-1 flex items-center gap-1.5 text-left hover:bg-sidebar-accent/50 px-1 py-0.5 -mx-1 min-w-0"
										>
											<WorkspaceIcon
												icon={workspace.icon}
												color={workspace.color}
												className={cn("flex-shrink-0", sizeClasses.workspaceIcon)}
											/>
											<span
												className={cn(
													"font-medium text-foreground truncate",
													sizeClasses.text,
												)}
											>
												{workspace.name}
											</span>
											<RoleBadge
												role={workspace.my_role}
												color={workspace.color}
											/>
											<span className={cn("text-muted-foreground", sizeClasses.countText)}>
												{workspace.member_count}
											</span>
										</button>
										{/* Action buttons - visible on hover */}
										{canManage && (
											<>
												<button
													type="button"
													onClick={() => onManageWorkspace(workspace)}
													className={cn(
														"text-muted-foreground hover:text-foreground hover:bg-sidebar-accent opacity-100 md:opacity-0 md:group-hover:opacity-100 transition-opacity",
														sizeClasses.buttonSize,
													)}
													title={t("sharedWorkspaces.settings", "Settings")}
												>
													<Settings className={sizeClasses.iconSize} />
												</button>
												{onNewProjectInWorkspace && (
													<button
														type="button"
														onClick={() => onNewProjectInWorkspace(workspace)}
														className={cn(
															"text-muted-foreground hover:text-foreground hover:bg-sidebar-accent opacity-100 md:opacity-0 md:group-hover:opacity-100 transition-opacity",
															sizeClasses.buttonSize,
														)}
														title={t("sharedWorkspaces.newProject", "New project")}
													>
														<FolderPlus className={sizeClasses.iconSize} />
													</button>
												)}
											</>
										)}
									</div>
								</ContextMenuTrigger>
								<ContextMenuContent>
									{onNewProjectInWorkspace && (
										<ContextMenuItem
											onClick={() => onNewProjectInWorkspace(workspace)}
										>
											<FolderPlus className="w-4 h-4 mr-2" />
											{t("sharedWorkspaces.newProject", "New project")}
										</ContextMenuItem>
									)}
									<ContextMenuSeparator />
									<ContextMenuItem
										onClick={() => onManageMembers(workspace)}
									>
										<UserPlus className="w-4 h-4 mr-2" />
										{t("sharedWorkspaces.manageMembers", "Members")}
									</ContextMenuItem>
									{canManage && (
										<>
											<ContextMenuItem
												onClick={() => onManageWorkspace(workspace)}
											>
												<Pencil className="w-4 h-4 mr-2" />
												{t("common.edit", "Edit")}
											</ContextMenuItem>
											{workspace.my_role === "owner" && (
												<>
													<ContextMenuSeparator />
													<ContextMenuItem
														variant="destructive"
														onClick={() => onDeleteWorkspace(workspace)}
													>
														<Trash2 className="w-4 h-4 mr-2" />
														{t("common.delete", "Delete")}
													</ContextMenuItem>
												</>
											)}
										</>
									)}
								</ContextMenuContent>
							</ContextMenu>

							{/* Expanded: show workdirs with sessions */}
							{isExpanded && (
								<div className="pb-1.5">
									<WorkdirList
										workspace={workspace}
										isMobile={isMobile}
										onSelectWorkdir={onSelectWorkdir}
										activeSessions={runnerSessions?.filter(
											(s) => s.shared_workspace_id === workspace.id,
										)}
										busySessions={busySessions}
										onSessionClick={onSessionClick}
									/>
								</div>
							)}
						</div>
					);
				})}
			</div>
		</div>
	);
});
