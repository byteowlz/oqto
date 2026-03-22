/**
 * Sidebar section displaying shared workspaces the user belongs to.
 * Each workspace is a collapsible top-level entry (like a project group).
 * Under each workspace, workdirs appear as project folders, and sessions
 * render identically to personal sessions (same metadata, styling, etc.).
 */
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
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type {
	SharedWorkspaceInfo,
	SharedWorkspaceWorkdir,
} from "@/lib/api/shared-workspaces";
import {
	deleteSharedWorkspaceWorkdir,
	listWorkdirs,
} from "@/lib/api/shared-workspaces";
import { listChatHistory, triggerChatHistoryBackfill } from "@/lib/api/chat";
import type { ChatSession } from "@/lib/api/chat";
import {
	formatSessionDate,
	formatTempId,
	getDisplayPiTitle,
	getTempIdFromSession,
} from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	ChevronDown,
	ChevronRight,
	Copy,
	FolderKanban,
	FolderPlus,
	Loader2,
	MessageSquare,
	MoreHorizontal,
	Pencil,
	Pin,
	Plus,
	RefreshCw,
	Settings,
	Trash2,
	UserPlus,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import { setSharedWorkspaceSessionId } from "@/components/contexts/chat-context";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { WorkspaceIcon } from "./WorkspaceIcon";

function hasString(
	collection: Set<string> | string[] | null | undefined,
	value: string,
): boolean {
	if (!collection) return false;
	return Array.isArray(collection)
		? collection.includes(value)
		: collection.has(value);
}

export interface SidebarSharedWorkspacesProps {
	sharedWorkspaces: SharedWorkspaceInfo[];
	expandedWorkspaces: Set<string> | string[];
	toggleWorkspaceExpanded: (workspaceId: string) => void;
	onNewSharedWorkspace: () => void;
	onManageWorkspace: (workspace: SharedWorkspaceInfo) => void;
	onManageMembers: (workspace: SharedWorkspaceInfo) => void;
	onNewChatInWorkspace: (workspace: SharedWorkspaceInfo) => void;
	onNewProjectInWorkspace?: (workspace: SharedWorkspaceInfo) => void;
	onDeleteWorkspace: (workspace: SharedWorkspaceInfo) => void;
	onSelectWorkdir?: (
		workspace: SharedWorkspaceInfo,
		workdir: SharedWorkspaceWorkdir,
	) => void;
	/** Full chat history from context (includes optimistic sessions). */
	chatHistory: ChatSession[];
	runnerSessions?: Array<{
		session_id: string;
		state: string;
		cwd: string;
		last_activity: number;
		shared_workspace_id?: string;
	}>;
	busySessions?: Set<string> | string[];
	selectedChatSessionId: string | null;
	onSessionClick?: (session: ChatSession, sharedWorkspaceId: string) => void;
	onRenameSession?: (sessionId: string) => void;
	onDeleteSession?: (sessionId: string) => Promise<boolean | void> | boolean | void;
	onPinSession?: (sessionId: string) => void;
	pinnedSessions?: Set<string> | string[];
	onPinProject?: (projectKey: string) => void;
	onRenameProject?: (projectKey: string, currentName: string) => void;
	onDeleteProject?: (projectKey: string, projectName: string) => void;
	pinnedProjects?: Set<string> | string[];
	isMobile?: boolean;
}

/** Workdir content: folders and sessions, matching personal sidebar style exactly. */
function WorkspaceContent({
	workspace,
	workspaceColor,
	isMobile,
	sizeClasses,
	onSelectWorkdir,
	onNewChatInWorkdir,
	chatHistory,
	busySessions,
	selectedChatSessionId,
	onSessionClick,
	onRenameSession,
	onDeleteSession,
	onPinSession,
	pinnedSessions,
	onPinProject,
	onRenameProject,
	onDeleteProject,
	pinnedProjects,
	expandedFolders,
	toggleFolderExpanded,
}: {
	workspace: SharedWorkspaceInfo;
	workspaceColor: string;
	isMobile: boolean;
	sizeClasses: SizeClasses;
	onSelectWorkdir?: (
		workspace: SharedWorkspaceInfo,
		workdir: SharedWorkspaceWorkdir,
	) => void;
	onNewChatInWorkdir?: (
		workspace: SharedWorkspaceInfo,
		workdir: SharedWorkspaceWorkdir,
	) => void;
	chatHistory: ChatSession[];
	busySessions?: Set<string> | string[];
	selectedChatSessionId: string | null;
	onSessionClick?: (session: ChatSession, sharedWorkspaceId: string) => void;
	onRenameSession?: (sessionId: string) => void;
	onDeleteSession?: (sessionId: string) => Promise<boolean | void> | boolean | void;
	onPinSession?: (sessionId: string) => void;
	pinnedSessions?: Set<string> | string[];
	onPinProject?: (projectKey: string) => void;
	onRenameProject?: (projectKey: string, currentName: string) => void;
	onDeleteProject?: (projectKey: string, projectName: string) => void;
	pinnedProjects?: Set<string> | string[];
	expandedFolders: Set<string>;
	toggleFolderExpanded: (key: string) => void;
}) {
	const { t } = useTranslation();
	const [workdirs, setWorkdirs] = useState<SharedWorkspaceWorkdir[]>([]);
	const [fetchedSessions, setFetchedSessions] = useState<ChatSession[]>([]);
	const [loading, setLoading] = useState(true);
	const [pendingDeleteWorkdir, setPendingDeleteWorkdir] =
		useState<SharedWorkspaceWorkdir | null>(null);
	const [isDeletingWorkdir, setIsDeletingWorkdir] = useState(false);
	const [pendingDeleteSession, setPendingDeleteSession] = useState<{
		id: string;
		title: string;
	} | null>(null);
	const [isDeletingSession, setIsDeletingSession] = useState(false);
	const [hiddenSessionIds, setHiddenSessionIds] = useState<Set<string>>(
		() => new Set(),
	);

	useEffect(() => {
		let cancelled = false;
		setLoading(true);
		setWorkdirs([]);
		setFetchedSessions([]);

		const loadWorkdirs = async () => {
			try {
				const wdData = await listWorkdirs(workspace.id);
				if (cancelled) return;
				setWorkdirs(Array.isArray(wdData) ? wdData : []);
			} catch {
				if (!cancelled) {
					setWorkdirs([]);
					toast.error("Failed to load shared workspace projects");
				}
			} finally {
				if (!cancelled) setLoading(false);
			}
		};

		const loadSessions = async () => {
			try {
				const sessionData = await listChatHistory({
					shared_workspace_id: workspace.id,
				});
				if (cancelled) return;
				const safeSessions = Array.isArray(sessionData) ? sessionData : [];
				const taggedSessions = safeSessions.map((s) => ({
					...s,
					shared_workspace_id: workspace.id,
				}));
				for (const s of taggedSessions) {
					setSharedWorkspaceSessionId(s.id, workspace.id);
				}
				setFetchedSessions(taggedSessions);
			} catch {
				if (!cancelled) {
					setFetchedSessions([]);
				}
			}
		};

		void loadWorkdirs();
		void loadSessions();

		return () => {
			cancelled = true;
		};
	}, [workspace.id]);

	// Merge fetched sessions with optimistic sessions from chatHistory.
	// Optimistic sessions (created via +) appear in chatHistory before they
	// exist in hstry. We include them if their workspace_path falls under
	// this workspace's path.
	const workspacePath = workspace.path.replace(/\/$/, "");
	const hstrySessions = useMemo(() => {
		const byId = new Map(
			fetchedSessions
				.filter((s) => !hiddenSessionIds.has(s.id))
				.map((s) => [s.id, s]),
		);
		// Add optimistic sessions from chatHistory that match this workspace
		for (const s of chatHistory) {
			if (hiddenSessionIds.has(s.id)) continue;
			if (byId.has(s.id)) continue;
			if (s.shared_workspace_id === workspace.id) {
				byId.set(s.id, s);
				continue;
			}
			const wp = s.workspace_path?.replace(/\/$/, "");
			if (wp && (wp === workspacePath || wp.startsWith(`${workspacePath}/`))) {
				byId.set(s.id, s);
			}
		}
		return Array.from(byId.values());
	}, [fetchedSessions, chatHistory, hiddenSessionIds, workspace.id, workspacePath]);

	// Group sessions by workdir path
	const sessionsByWorkdir = useMemo(() => {
		const map = new Map<string, ChatSession[]>();
		for (const s of hstrySessions) {
			const wp = s.workspace_path?.replace(/\/$/, "");
			for (const wd of workdirs) {
				const normalizedWdPath = wd.path.replace(/\/$/, "");
				if (
					wp === normalizedWdPath ||
					wp?.startsWith(`${normalizedWdPath}/`)
				) {
					const existing = map.get(wd.path) ?? [];
					existing.push(s);
					map.set(wd.path, existing);
					break;
				}
			}
		}
		// Sort each workdir's sessions newest-first
		for (const sessions of map.values()) {
			sessions.sort(
				(a, b) => (b.updated_at ?? b.created_at) - (a.updated_at ?? a.created_at),
			);
		}
		return map;
	}, [hstrySessions, workdirs]);

	if (loading) {
		return (
			<div
				className={cn(
					"px-5 py-1 text-muted-foreground/60",
					isMobile ? "text-xs" : "text-[10px]",
				)}
			>
				...
			</div>
		);
	}

	if (workdirs.length === 0) {
		return (
			<div
				className={cn(
					"px-5 py-1 text-muted-foreground/60 italic",
					isMobile ? "text-xs" : "text-[10px]",
				)}
			>
				No projects yet
			</div>
		);
	}

	const confirmDeleteWorkdir = async () => {
		if (!pendingDeleteWorkdir) return;
		setIsDeletingWorkdir(true);
		try {
			await deleteSharedWorkspaceWorkdir(workspace.id, pendingDeleteWorkdir.path);
			setWorkdirs((prev) => prev.filter((d) => d.path !== pendingDeleteWorkdir.path));
			setFetchedSessions((prev) =>
				prev.filter((s) => {
					const wp = s.workspace_path ?? "";
					return !(
						wp === pendingDeleteWorkdir.path ||
						wp.startsWith(`${pendingDeleteWorkdir.path}/`)
					);
				}),
			);
			onDeleteProject?.(pendingDeleteWorkdir.path, pendingDeleteWorkdir.name);
			toast.success(`Deleted project "${pendingDeleteWorkdir.name}"`);
		} catch (e) {
			toast.error(
				e instanceof Error ? e.message : "Failed to delete shared project",
			);
		} finally {
			setIsDeletingWorkdir(false);
			setPendingDeleteWorkdir(null);
		}
	};

	const backfillWorkdirSessions = async (workdirPath: string) => {
		try {
			const result = await triggerChatHistoryBackfill({
				workspace: workdirPath,
				shared_workspace_id: workspace.id,
			});
			const sessionData = await listChatHistory({
				shared_workspace_id: workspace.id,
			});
			const safeSessions = Array.isArray(sessionData) ? sessionData : [];
			setFetchedSessions(
				safeSessions.map((s) => ({ ...s, shared_workspace_id: workspace.id })),
			);
			toast.success(
				`Backfill complete: repaired ${result.repaired_conversations}, scanned ${result.scanned_files}`,
			);
		} catch (e) {
			toast.error(e instanceof Error ? e.message : "Backfill failed");
		}
	};

	const confirmDeleteSession = async () => {
		if (!pendingDeleteSession || !onDeleteSession) return;
		const deletingId = pendingDeleteSession.id;
		setIsDeletingSession(true);
		setHiddenSessionIds((prev) => new Set(prev).add(deletingId));
		try {
			setSharedWorkspaceSessionId(deletingId, workspace.id);
			const result = await Promise.resolve(onDeleteSession(deletingId));
			if (result === false) {
				setHiddenSessionIds((prev) => {
					const next = new Set(prev);
					next.delete(deletingId);
					return next;
				});
				return;
			}
			setFetchedSessions((prev) => prev.filter((s) => s.id !== deletingId));
		} catch {
			setHiddenSessionIds((prev) => {
				const next = new Set(prev);
				next.delete(deletingId);
				return next;
			});
		} finally {
			setIsDeletingSession(false);
			setPendingDeleteSession(null);
		}
	};

	return (
		<>
		<div className="space-y-0.5 pb-1">
			{workdirs.map((wd) => {
				const wdSessions = sessionsByWorkdir.get(wd.path) ?? [];
				const folderKey = `${workspace.id}:${wd.path}`;
				const isFolderExpanded = expandedFolders.has(folderKey);
				const isPinnedProject = hasString(pinnedProjects, wd.path);

				return (
					<div
						key={wd.path}
						className="border-b border-sidebar-border/50 last:border-b-0"
					>
						{/* Folder header - identical to personal project header, with context menu */}
						<ContextMenu>
							<ContextMenuTrigger asChild>
								<div className="flex items-center gap-1 px-1 py-1.5 group">
									<button
										type="button"
										onClick={() => toggleFolderExpanded(folderKey)}
										className="flex items-center gap-1.5 text-left hover:bg-sidebar-accent/50 px-1 py-0.5 -mx-1"
									>
										{isFolderExpanded ? (
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
										onClick={() => toggleFolderExpanded(folderKey)}
										className="flex-1 flex items-center gap-1.5 text-left hover:bg-sidebar-accent/50 px-1 py-0.5 -mx-1"
									>
										<FolderKanban
											className={cn(
												"flex-shrink-0",
												sizeClasses.projectIcon,
											)}
											style={{ color: workspaceColor }}
										/>
										{isPinnedProject && (
											<Pin className="w-3 h-3 flex-shrink-0 text-primary/70" />
										)}
										<span
											className={cn(
												"font-medium text-foreground truncate",
												sizeClasses.projectText,
											)}
										>
											{wd.name}
										</span>
										<span className="text-[10px] text-muted-foreground">
											({wdSessions.length})
										</span>
									</button>
									{/* New chat in this workdir */}
									<button
										type="button"
										onClick={() => onSelectWorkdir?.(workspace, wd)}
										className={cn(
											"text-muted-foreground hover:text-primary hover:bg-sidebar-accent opacity-100 md:opacity-0 md:group-hover:opacity-100 transition-opacity",
											sizeClasses.buttonSize,
										)}
										title="New chat"
									>
										<Plus className={sizeClasses.iconSize} />
									</button>
									<DropdownMenu>
										<DropdownMenuTrigger asChild>
											<button
												type="button"
												className={cn(
													"text-muted-foreground hover:text-primary hover:bg-sidebar-accent opacity-100 md:opacity-0 md:group-hover:opacity-100 transition-opacity",
													sizeClasses.buttonSize,
												)}
												title={t("common.actions", "Actions")}
											>
												<MoreHorizontal className={sizeClasses.iconSize} />
											</button>
										</DropdownMenuTrigger>
										<DropdownMenuContent align="end">
											<DropdownMenuItem onClick={() => onNewChatInWorkdir?.(workspace, wd)}>
												<Plus className="w-4 h-4 mr-2" />
												{t("chat.newChat", "New chat")}
											</DropdownMenuItem>
											<DropdownMenuItem onClick={() => navigator.clipboard.writeText(wd.path)}>
												<Copy className="w-4 h-4 mr-2" />
												{t("common.copyPath", "Copy path")}
											</DropdownMenuItem>
											<DropdownMenuItem onClick={() => void backfillWorkdirSessions(wd.path)}>
												<RefreshCw className="w-4 h-4 mr-2" />
												Backfill sessions
											</DropdownMenuItem>
										</DropdownMenuContent>
									</DropdownMenu>
								</div>
							</ContextMenuTrigger>
							<ContextMenuContent>
								<ContextMenuItem
									onClick={() => onNewChatInWorkdir?.(workspace, wd)}
								>
									<Plus className="w-4 h-4 mr-2" />
									{t("chat.newChat", "New chat")}
								</ContextMenuItem>
								<ContextMenuItem
									onClick={() => {
										navigator.clipboard.writeText(wd.path);
									}}
								>
									<Copy className="w-4 h-4 mr-2" />
									{t("common.copyPath", "Copy path")}
								</ContextMenuItem>
								<ContextMenuItem onClick={() => void backfillWorkdirSessions(wd.path)}>
									<RefreshCw className="w-4 h-4 mr-2" />
									Backfill sessions
								</ContextMenuItem>
								{onPinProject && (
									<ContextMenuItem onClick={() => onPinProject(wd.path)}>
										<Pin className="w-4 h-4 mr-2" />
										{isPinnedProject ? t("projects.unpin") : t("projects.pin")}
									</ContextMenuItem>
								)}
								{onRenameProject && (
									<ContextMenuItem onClick={() => onRenameProject(wd.path, wd.name)}>
										<Pencil className="w-4 h-4 mr-2" />
										{t("common.rename")}
									</ContextMenuItem>
								)}
								{(onPinProject || onRenameProject) && onDeleteProject && <ContextMenuSeparator />}
								{onDeleteProject && (
									<ContextMenuItem
										variant="destructive"
										onClick={() => setPendingDeleteWorkdir(wd)}
									>
										<Trash2 className="w-4 h-4 mr-2" />
										{t("common.delete")}
									</ContextMenuItem>
								)}
							</ContextMenuContent>
						</ContextMenu>

						{/* Sessions - identical to personal session items */}
						{isFolderExpanded && (
							<div className="space-y-0.5 pb-1">
								{wdSessions.map((session) => {
									const isSelected =
										selectedChatSessionId === session.id;
									const isBusy = hasString(busySessions, session.id);
									const formattedDate = session.updated_at
										? formatSessionDate(session.updated_at)
										: null;
									const tempId = formatTempId(getTempIdFromSession(session));
									const isPinned = hasString(pinnedSessions, session.id);

									return (
										<div
											key={session.id}
											className={isMobile ? "ml-4" : "ml-3"}
										>
											<ContextMenu>
												<ContextMenuTrigger asChild>
													<div
														className={cn(
															"w-full px-2 text-left transition-colors flex items-start gap-1.5 cursor-pointer",
															isMobile ? "py-2" : "py-1",
															isSelected
																? "bg-primary/15 border border-primary text-foreground"
																: "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
														)}
														onClick={() => onSessionClick?.(session, workspace.id)}
														onKeyDown={(e) => {
															if (e.key === "Enter" || e.key === " ") {
																onSessionClick?.(session, workspace.id);
															}
														}}
														role="button"
														tabIndex={0}
													>
														<MessageSquare
															className={cn(
																"mt-0.5 flex-shrink-0",
																isMobile ? "w-4 h-4" : "w-3 h-3",
															)}
															style={{ color: workspaceColor }}
														/>
														<div className="flex-1 min-w-0 text-left">
															<div className="flex items-center gap-1">
																{isPinned && (
																	<Pin className="w-3 h-3 flex-shrink-0 text-primary/70" />
																)}
																<span
																	className={cn(
																		"truncate font-medium",
																		sizeClasses.sessionText,
																	)}
																>
																	{getDisplayPiTitle(session)}
																</span>
																{isBusy && (
																	<Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
																)}
															</div>
															{formattedDate && (
																<div
																	className={cn(
																		"text-muted-foreground mt-0.5",
																		sizeClasses.dateText,
																	)}
																>
																	{formattedDate}
																</div>
															)}
														</div>
													</div>
												</ContextMenuTrigger>
												<ContextMenuContent>
													{tempId && (
														<ContextMenuItem
															onClick={() => {
																navigator.clipboard.writeText(tempId);
															}}
														>
															<Copy className="w-4 h-4 mr-2" />
															{tempId}
														</ContextMenuItem>
													)}
													<ContextMenuItem
														onClick={() => {
															navigator.clipboard.writeText(session.id);
														}}
													>
														<Copy className="w-4 h-4 mr-2" />
														{session.id.slice(0, 16)}...
													</ContextMenuItem>
													<ContextMenuSeparator />
													{onPinSession && (
														<ContextMenuItem
															onClick={() => onPinSession(session.id)}
														>
															<Pin className="w-4 h-4 mr-2" />
															{isPinned ? t("projects.unpin") : t("projects.pin")}
														</ContextMenuItem>
													)}
													{onRenameSession && (
														<ContextMenuItem
															onClick={() => onRenameSession(session.id)}
														>
															<Pencil className="w-4 h-4 mr-2" />
															{t("common.rename")}
														</ContextMenuItem>
													)}
													{(onPinSession || onRenameSession) && onDeleteSession && (
														<ContextMenuSeparator />
													)}
													{onDeleteSession && (
														<ContextMenuItem
															variant="destructive"
															onClick={() => {
																setPendingDeleteSession({
																	id: session.id,
																	title: getDisplayPiTitle(session),
																});
															}}
														>
															<Trash2 className="w-4 h-4 mr-2" />
															{t("common.delete")}
														</ContextMenuItem>
													)}
												</ContextMenuContent>
											</ContextMenu>
										</div>
									);
								})}
							</div>
						)}
					</div>
				);
			})}
		</div>
		<AlertDialog
			open={pendingDeleteWorkdir !== null}
			onOpenChange={(open) => {
				if (!open && !isDeletingWorkdir) {
					setPendingDeleteWorkdir(null);
				}
			}}
		>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>
						{t("common.delete", "Delete")} {pendingDeleteWorkdir?.name}
					</AlertDialogTitle>
					<AlertDialogDescription>
						This will permanently delete the shared project folder and all chats inside it.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel disabled={isDeletingWorkdir}>
						{t("common.cancel", "Cancel")}
					</AlertDialogCancel>
					<AlertDialogAction
						onClick={(e) => {
							e.preventDefault();
							void confirmDeleteWorkdir();
						}}
						disabled={isDeletingWorkdir}
					>
						{isDeletingWorkdir ? "Deleting..." : t("common.delete", "Delete")}
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
		<AlertDialog
			open={pendingDeleteSession !== null}
			onOpenChange={(open) => {
				if (!open && !isDeletingSession) {
					setPendingDeleteSession(null);
				}
			}}
		>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>
						{pendingDeleteSession?.title
							? t("sessions.deleteTitle", { title: pendingDeleteSession.title })
							: t("sessions.deleteChatTitle")}
					</AlertDialogTitle>
					<AlertDialogDescription>
						{t("sessions.deleteDescription")}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel disabled={isDeletingSession}>
						{t("common.cancel", "Cancel")}
					</AlertDialogCancel>
					<AlertDialogAction
						onClick={(e) => {
							e.preventDefault();
							void confirmDeleteSession();
						}}
						disabled={isDeletingSession}
					>
						{isDeletingSession ? "Deleting..." : t("common.delete", "Delete")}
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
		</>
	);
}

interface SizeClasses {
	headerText: string;
	iconSize: string;
	workspaceIcon: string;
	projectIcon: string;
	projectText: string;
	sessionText: string;
	text: string;
	buttonSize: string;
	countText: string;
	dateText: string;
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
	chatHistory,
	runnerSessions,
	busySessions,
	selectedChatSessionId,
	onSessionClick,
	onRenameSession,
	onDeleteSession,
	onPinSession,
	pinnedSessions,
	onPinProject,
	onRenameProject,
	onDeleteProject,
	pinnedProjects,
	isMobile = false,
}: SidebarSharedWorkspacesProps) {
	const { t } = useTranslation();
	const [expandedFolders, setExpandedFolders] = useState<Set<string>>(
		() => new Set(),
	);

	const toggleFolderExpanded = useCallback((key: string) => {
		setExpandedFolders((prev) => {
			const next = new Set(prev);
			if (next.has(key)) {
				next.delete(key);
			} else {
				next.add(key);
			}
			return next;
		});
	}, []);

	if (sharedWorkspaces.length === 0) {
		return null;
	}

	const sizeClasses: SizeClasses = isMobile
		? {
				headerText: "text-xs",
				iconSize: "w-4 h-4",
				workspaceIcon: "w-4 h-4",
				projectIcon: "w-4 h-4",
				projectText: "text-sm",
				sessionText: "text-sm",
				text: "text-sm",
				buttonSize: "p-1.5",
				countText: "text-xs",
				dateText: "text-[11px]",
			}
		: {
				headerText: "text-xs",
				iconSize: "w-3 h-3",
				workspaceIcon: "w-3.5 h-3.5",
				projectIcon: "w-3.5 h-3.5",
				projectText: "text-xs",
				sessionText: "text-xs",
				text: "text-xs",
				buttonSize: "p-1",
				countText: "text-[10px]",
				dateText: "text-[9px]",
			};

	return (
		<div className="space-y-0.5">
			{sharedWorkspaces.map((workspace) => {
				const isExpanded = hasString(expandedWorkspaces, workspace.id);
				const canManage =
					workspace.my_role === "owner" || workspace.my_role === "admin";

				return (
					<div
						key={workspace.id}
						className="border-b border-sidebar-border/50 last:border-b-0"
					>
						{/* Workspace header - top level, like a project group but with workspace icon/color */}
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
											className={cn(
												"flex-shrink-0",
												sizeClasses.workspaceIcon,
											)}
										/>
										<span
											className={cn(
												"font-medium text-foreground truncate",
												sizeClasses.text,
											)}
										>
											{workspace.name}
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
												title={t(
													"sharedWorkspaces.settings",
													"Settings",
												)}
											>
												<Settings className={sizeClasses.iconSize} />
											</button>
											{onNewProjectInWorkspace && (
												<button
													type="button"
													onClick={() =>
														onNewProjectInWorkspace(workspace)
													}
													className={cn(
														"text-muted-foreground hover:text-foreground hover:bg-sidebar-accent opacity-100 md:opacity-0 md:group-hover:opacity-100 transition-opacity",
														sizeClasses.buttonSize,
													)}
													title={t(
														"sharedWorkspaces.newProject",
														"New project",
													)}
												>
													<FolderPlus
														className={sizeClasses.iconSize}
													/>
												</button>
											)}
										</>
									)}
								</div>
							</ContextMenuTrigger>
							<ContextMenuContent>
								{onNewProjectInWorkspace && (
									<ContextMenuItem
										onClick={() =>
											onNewProjectInWorkspace(workspace)
										}
									>
										<FolderPlus className="w-4 h-4 mr-2" />
										{t(
											"sharedWorkspaces.newProject",
											"New project",
										)}
									</ContextMenuItem>
								)}
								<ContextMenuSeparator />
								<ContextMenuItem
									onClick={() => onManageMembers(workspace)}
								>
									<UserPlus className="w-4 h-4 mr-2" />
									{t(
										"sharedWorkspaces.manageMembers",
										"Members",
									)}
								</ContextMenuItem>
								{canManage && (
									<>
										<ContextMenuItem
											onClick={() =>
												onManageWorkspace(workspace)
											}
										>
											<Pencil className="w-4 h-4 mr-2" />
											{t("common.edit", "Edit")}
										</ContextMenuItem>
										{workspace.my_role === "owner" && (
											<>
												<ContextMenuSeparator />
												<ContextMenuItem
													variant="destructive"
													onClick={() =>
														onDeleteWorkspace(workspace)
													}
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

						{/* Expanded: workdirs as folders, sessions as items */}
						{isExpanded && (
							<WorkspaceContent
								workspace={workspace}
								workspaceColor={workspace.color}
								isMobile={isMobile}
								sizeClasses={sizeClasses}
								onSelectWorkdir={onSelectWorkdir}
								onNewChatInWorkdir={onSelectWorkdir}
								chatHistory={chatHistory}
								busySessions={busySessions}
								selectedChatSessionId={selectedChatSessionId}
								onSessionClick={onSessionClick}
								onRenameSession={onRenameSession}
								onDeleteSession={onDeleteSession}
								onPinSession={onPinSession}
								pinnedSessions={pinnedSessions}
								onPinProject={onPinProject}
								onRenameProject={onRenameProject}
								onDeleteProject={onDeleteProject}
								pinnedProjects={pinnedProjects}
								expandedFolders={expandedFolders}
								toggleFolderExpanded={toggleFolderExpanded}
							/>
						)}
					</div>
				);
			})}

			{/* Create new shared workspace button */}
			<button
				type="button"
				onClick={onNewSharedWorkspace}
				className={cn(
					"w-full flex items-center gap-1.5 px-2 py-1.5 text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded transition-colors",
					sizeClasses.text,
				)}
			>
				<Plus className={sizeClasses.iconSize} />
				<span>{t("sharedWorkspaces.create", "New shared workspace")}</span>
			</button>
		</div>
	);
});
