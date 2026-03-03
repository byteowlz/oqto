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

/** Fetches and displays workdirs for an expanded workspace */
function WorkdirList({
	workspace,
	isMobile,
	onSelectWorkdir,
}: {
	workspace: SharedWorkspaceInfo;
	isMobile: boolean;
	onSelectWorkdir?: (workspace: SharedWorkspaceInfo, workdir: SharedWorkspaceWorkdir) => void;
}) {
	const [workdirs, setWorkdirs] = useState<SharedWorkspaceWorkdir[]>([]);
	const [loading, setLoading] = useState(true);

	useEffect(() => {
		let cancelled = false;
		setLoading(true);
		listWorkdirs(workspace.id)
			.then((data) => {
				if (!cancelled) setWorkdirs(data);
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

	return (
		<div className="space-y-0.5">
			{workdirs.map((wd) => (
				<button
					key={wd.path}
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
				</button>
			))}
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

							{/* Expanded: show workdirs */}
							{isExpanded && (
								<div className="pb-1.5">
									<WorkdirList
										workspace={workspace}
										isMobile={isMobile}
										onSelectWorkdir={onSelectWorkdir}
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
