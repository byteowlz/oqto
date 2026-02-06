import {
	type AgentFilter,
	type SearchMode,
	SearchResults,
} from "@/components/search";
import { Button } from "@/components/ui/button";
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
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type {
	ChatSession,
	HstrySearchHit,
	ProjectLogo,
} from "@/lib/control-plane-client";
import {
	formatSessionDate,
	getReadableIdFromSession,
} from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import { DeleteConfirmDialog } from "@/src/routes/app-shell/dialogs/DeleteConfirmDialog";
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
	Loader2,
	MessageSquare,
	Pencil,
	Pin,
	Plus,
	Search,
	Trash2,
	X,
} from "lucide-react";
import {
	memo,
	useCallback,
	useDeferredValue,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";

export interface SessionsByProject {
	key: string;
	name: string;
	directory?: string;
	sessions: ChatSession[];
	logo?: ProjectLogo;
}

export interface SessionHierarchy {
	parentSessions: ChatSession[];
	childSessionsByParent: Map<string, ChatSession[]>;
}

export interface SidebarSessionsProps {
	locale: string;
	chatHistory: ChatSession[];
	sessionHierarchy: SessionHierarchy;
	sessionsByProject: SessionsByProject[];
	filteredSessions: ChatSession[];
	selectedChatSessionId: string | null;
	busySessions: Set<string>;
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
	onNewChat: () => void;
	onNewProject: () => void;
	onProjectClear: () => void;
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
	isMobile?: boolean;
}

export const SidebarSessions = memo(function SidebarSessions({
	locale,
	chatHistory,
	sessionHierarchy,
	sessionsByProject,
	filteredSessions,
	selectedChatSessionId,
	busySessions,
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
	onNewChat,
	onNewProject,
	onProjectClear,
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
	isMobile = false,
}: SidebarSessionsProps) {
	// Session search
	const [sessionSearch, setSessionSearch] = useState("");
	const deferredSearch = useDeferredValue(sessionSearch);
	const [searchMode, setSearchMode] = useState<SearchMode>("sessions");
	const [agentFilter, setAgentFilter] = useState<AgentFilter>("all");
	const [selectedSessionIds, setSelectedSessionIds] = useState<Set<string>>(
		() => new Set(),
	);
	const [hiddenSessionIds, setHiddenSessionIds] = useState<Set<string>>(
		() => new Set(),
	);
	const [bulkDeleteOpen, setBulkDeleteOpen] = useState(false);
	const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
	const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
	const [pendingDeleteTitle, setPendingDeleteTitle] = useState("");
	const lastSelectedIndexRef = useRef<number | null>(null);
	const isFilteringSessions =
		searchMode === "sessions" && deferredSearch.trim().length > 0;

	const ensureBaseSelection = useCallback(
		(prev: Set<string>) => {
			const next = new Set(prev);
			if (selectedChatSessionId && !next.has(selectedChatSessionId)) {
				next.add(selectedChatSessionId);
			}
			return next;
		},
		[selectedChatSessionId],
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

	const sizeClasses = isMobile
		? {
				searchInput: "pl-12 pr-10 py-2 text-sm",
				headerText: "text-xs",
				sessionCount: "text-xs",
				projectIcon: "w-4 h-4",
				projectText: "text-sm",
				sessionText: "text-sm",
				buttonSize: "p-1.5",
				iconSize: "w-4 h-4",
				dateText: "text-[11px]",
			}
		: {
				searchInput: "pl-12 pr-8 py-1.5 text-xs",
				headerText: "text-xs",
				sessionCount: "text-xs",
				projectIcon: "w-3.5 h-3.5",
				projectText: "text-xs",
				sessionText: "text-xs",
				buttonSize: "p-1",
				iconSize: "w-3 h-3",
				dateText: "text-[9px]",
			};

	const visibleSessionIds = useMemo(() => {
		const ids: string[] = [];
		for (const project of sessionsByProject) {
			const isProjectExpanded =
				deferredSearch.trim().length > 0 || expandedProjects.has(project.key);
			if (!isProjectExpanded) continue;
			for (const session of project.sessions) {
				if (hiddenSessionIds.has(session.id)) continue;
				ids.push(session.id);
				const children =
					sessionHierarchy.childSessionsByParent.get(session.id) || [];
				const hasChildren = children.length > 0;
				const isExpanded = expandedSessions.has(session.id);
				if (hasChildren && isExpanded) {
					for (const child of children) {
						if (hiddenSessionIds.has(child.id)) continue;
						ids.push(child.id);
					}
				}
			}
		}
		return ids;
	}, [
		deferredSearch,
		expandedProjects,
		expandedSessions,
		hiddenSessionIds,
		sessionHierarchy.childSessionsByParent,
		sessionsByProject,
	]);

	const sessionIndexById = useMemo(() => {
		const map = new Map<string, number>();
		visibleSessionIds.forEach((id, idx) => map.set(id, idx));
		return map;
	}, [visibleSessionIds]);

	useEffect(() => {
		setSelectedSessionIds((prev) => {
			if (prev.size === 0) return prev;
			const visible = new Set(visibleSessionIds);
			const next = new Set<string>();
			for (const id of prev) {
				if (visible.has(id)) next.add(id);
			}
			return next;
		});
		lastSelectedIndexRef.current = null;
	}, [visibleSessionIds]);

	const handleSessionRowClick = (e: React.MouseEvent, sessionId: string) => {
		const index = sessionIndexById.get(sessionId);
		const baseIndex =
			lastSelectedIndexRef.current ??
			(selectedChatSessionId
				? (sessionIndexById.get(selectedChatSessionId) ?? null)
				: null);
		const hasRange = e.shiftKey && baseIndex !== null && index !== undefined;
		const isToggle = e.metaKey || e.ctrlKey;
		if (hasRange && baseIndex != null && index != null) {
			const start = Math.min(baseIndex, index);
			const end = Math.max(baseIndex, index);
			const rangeIds = visibleSessionIds.slice(start, end + 1);
			setSelectedSessionIds((prev) => {
				const next = ensureBaseSelection(prev);
				for (const id of rangeIds) next.add(id);
				return next;
			});
		} else if (isToggle) {
			setSelectedSessionIds((prev) => {
				const next = ensureBaseSelection(prev);
				if (next.has(sessionId)) {
					next.delete(sessionId);
				} else {
					next.add(sessionId);
				}
				return next;
			});
		} else {
			if (selectedSessionIds.size > 0) {
				setSelectedSessionIds(new Set());
			}
			onSessionClick(sessionId);
		}

		if (index !== undefined) {
			lastSelectedIndexRef.current = index;
		}
	};

	const handleBulkDelete = async () => {
		if (selectedSessionIds.size === 0) return;
		const ids = Array.from(selectedSessionIds);
		setSelectedSessionIds(new Set());
		setHiddenSessionIds((prev) => {
			const next = new Set(prev);
			for (const id of ids) next.add(id);
			return next;
		});
		let failed: string[] = [];
		try {
			const result = await onBulkDeleteSessions(ids);
			if (Array.isArray(result)) {
				failed = result;
			}
		} catch {
			failed = ids;
		}
		if (failed.length > 0) {
			setHiddenSessionIds((prev) => {
				const next = new Set(prev);
				for (const id of failed) next.delete(id);
				return next;
			});
		}
	};

	const handleDeleteSession = async (sessionId: string) => {
		const session = filteredSessions.find((item) => item.id === sessionId);
		setPendingDeleteId(sessionId);
		setPendingDeleteTitle(session?.title ?? "");
		setDeleteDialogOpen(true);
	};

	const handleBulkDeleteRequest = () => {
		if (selectedSessionIds.size === 0) return;
		setBulkDeleteOpen(true);
	};

	return (
		<div className="flex-1 min-h-0 flex flex-col overflow-x-hidden">
			<DeleteConfirmDialog
				open={deleteDialogOpen}
				onOpenChange={(open) => {
					setDeleteDialogOpen(open);
					if (!open) {
						setPendingDeleteId(null);
						setPendingDeleteTitle("");
					}
				}}
				onConfirm={async () => {
					if (!pendingDeleteId) return;
					await Promise.resolve(onDeleteSession(pendingDeleteId));
					setDeleteDialogOpen(false);
					setPendingDeleteId(null);
					setPendingDeleteTitle("");
				}}
				locale={locale}
				title={
					locale === "de"
						? pendingDeleteTitle
							? `"${pendingDeleteTitle}" loschen?`
							: "Chat loschen?"
						: pendingDeleteTitle
							? `Delete "${pendingDeleteTitle}"?`
							: "Delete chat?"
				}
			/>
			<DeleteConfirmDialog
				open={bulkDeleteOpen}
				onOpenChange={setBulkDeleteOpen}
				onConfirm={() => {
					setBulkDeleteOpen(false);
					handleBulkDelete();
				}}
				locale={locale}
				title={
					locale === "de"
						? `${selectedSessionIds.size} Chats loschen?`
						: `Delete ${selectedSessionIds.size} chats?`
				}
				description={
					locale === "de"
						? "Diese Aktion kann nicht ruckgangig gemacht werden. Alle ausgewahlten Chats werden dauerhaft geloscht."
						: "This action cannot be undone. All selected chats will be permanently deleted."
				}
			/>
			{/* Sticky header section - Search, Default Chat, Sessions header */}
			<div className="flex-shrink-0 space-y-0.5 px-1">
				{/* Search input with mode dropdown */}
				<div className="relative mb-2 px-1">
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
								className={cn(searchMode === "sessions" && "bg-accent")}
							>
								<Search className="w-3.5 h-3.5 mr-2" />
								{locale === "de" ? "Sitzungen filtern" : "Filter sessions"}
							</DropdownMenuItem>
							<DropdownMenuItem
								onClick={() => setSearchMode("messages")}
								className={cn(searchMode === "messages" && "bg-accent")}
							>
								<MessageSquare className="w-3.5 h-3.5 mr-2" />
								{locale === "de" ? "Nachrichten suchen" : "Search messages"}
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
										onClick={() => setAgentFilter("pi_agent")}
										className={cn(agentFilter === "pi_agent" && "bg-accent")}
									>
										{locale === "de" ? "Nur Chat" : "Chat only"}
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
						className={cn(
							"w-full bg-sidebar-accent/50 border border-sidebar-border rounded placeholder:text-muted-foreground/50 focus:outline-none focus:border-primary/50",
							sizeClasses.searchInput,
						)}
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
				{selectedSessionIds.size > 0 && (
					<div className="flex items-center gap-2 bg-primary/10 border border-primary/20 rounded px-2 py-1 mx-1 mt-1">
						<span className="text-xs font-medium text-primary">
							{selectedSessionIds.size}
						</span>
						<div className="flex-1 mr-1" />
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={handleBulkDeleteRequest}
							className="h-6 px-2 text-xs"
						>
							<Trash2 className="w-3 h-3 mr-1" />
							{locale === "de" ? "Loschen" : "Delete"}
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => setSelectedSessionIds(new Set())}
							className="h-6 w-6 p-0"
							title={locale === "de" ? "Auswahl loschen" : "Clear selection"}
						>
							<X className="w-3 h-3" />
						</Button>
					</div>
				)}
				{/* Sessions header - between search and chat list */}
				<div className="flex items-center justify-between gap-2 py-1.5 px-1">
					<div className="flex items-center gap-2">
						<span
							className={cn(
								"uppercase tracking-wide text-muted-foreground",
								sizeClasses.headerText,
							)}
						>
							{locale === "de" ? "Sitzungen" : "Sessions"}
						</span>
						<span
							className={cn(
								"text-muted-foreground/50",
								sizeClasses.sessionCount,
							)}
						>
							({filteredSessions.length}
							{deferredSearch ? `/${chatHistory.length}` : ""})
						</span>
					</div>
					<div className="flex items-center gap-1">
						<button
							type="button"
							onClick={onNewChat}
							className={cn(
								"text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded",
								sizeClasses.buttonSize,
							)}
							title={locale === "de" ? "Neue Sitzung" : "New session"}
						>
							<Plus className={sizeClasses.iconSize} />
						</button>
						<button
							type="button"
							onClick={onNewProject}
							className={cn(
								"text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded",
								sizeClasses.buttonSize,
							)}
							title={locale === "de" ? "Neues Projekt" : "New project"}
						>
							<FolderPlus className={sizeClasses.iconSize} />
						</button>
						{selectedProjectLabel && (
							<button
								type="button"
								onClick={onProjectClear}
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
									className={cn(
										"text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded",
										sizeClasses.buttonSize,
									)}
									title={locale === "de" ? "Sortieren" : "Sort"}
								>
									{projectSortAsc ? (
										<ArrowUp className={sizeClasses.iconSize} />
									) : (
										<ArrowDown className={sizeClasses.iconSize} />
									)}
								</button>
							</DropdownMenuTrigger>
							<DropdownMenuContent align="end" className="w-36">
								<DropdownMenuItem
									onClick={() => setProjectSortBy("date")}
									className={cn(projectSortBy === "date" && "bg-accent")}
								>
									<Clock className="w-3.5 h-3.5 mr-2" />
									{locale === "de" ? "Datum" : "Date"}
								</DropdownMenuItem>
								<DropdownMenuItem
									onClick={() => setProjectSortBy("name")}
									className={cn(projectSortBy === "name" && "bg-accent")}
								>
									<ArrowUpDown className="w-3.5 h-3.5 mr-2" />
									{locale === "de" ? "Name" : "Name"}
								</DropdownMenuItem>
								<DropdownMenuItem
									onClick={() => setProjectSortBy("sessions")}
									className={cn(projectSortBy === "sessions" && "bg-accent")}
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
			<div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden space-y-1 px-1">
				{/* Message search results (when in messages mode with query) */}
				{searchMode === "messages" && sessionSearch.trim() ? (
					<SearchResults
						query={sessionSearch}
						agentFilter={agentFilter}
						locale={locale}
						onResultClick={onSearchResultClick}
						extraHits={messageSearchExtraHits}
						className={isMobile ? "mb-2" : undefined}
					/>
				) : (
					<>
						{filteredSessions.length === 0 && deferredSearch && (
							<div
								className={cn(
									"text-muted-foreground/50 text-center py-4",
									sizeClasses.sessionText,
								)}
							>
								{locale === "de" ? "Keine Ergebnisse" : "No results"}
							</div>
						)}
						{sessionsByProject.map((project) => {
							// Auto-expand all when searching
							const isProjectExpanded =
								deferredSearch || expandedProjects.has(project.key);
							const isProjectPinned = pinnedProjects.includes(project.key);
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
													onClick={() => toggleProjectExpanded(project.key)}
													className="flex-1 flex items-center gap-1.5 text-left hover:bg-sidebar-accent/50 px-1 py-0.5 -mx-1"
												>
													{isProjectExpanded ? (
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
													{isProjectPinned && (
														<Pin
															className={cn(
																"text-primary/70 flex-shrink-0",
																sizeClasses.iconSize,
															)}
														/>
													)}
													<FolderKanban
														className={cn(
															"text-primary/70 flex-shrink-0",
															sizeClasses.projectIcon,
														)}
													/>
													<span
														className={cn(
															"font-medium text-foreground truncate",
															sizeClasses.projectText,
														)}
													>
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
															onNewChatInProject(project.directory as string)
														}
														className={cn(
															"text-muted-foreground hover:text-primary hover:bg-sidebar-accent opacity-0 group-hover:opacity-100 transition-opacity",
															sizeClasses.buttonSize,
														)}
														title={
															locale === "de"
																? "Neuer Chat in diesem Projekt"
																: "New chat in this project"
														}
													>
														<Plus className={sizeClasses.iconSize} />
													</button>
												) : null}
											</div>
										</ContextMenuTrigger>
										<ContextMenuContent>
											{project.directory && (
												<>
													<ContextMenuItem
														onClick={() =>
															onNewChatInProject(project.directory as string)
														}
													>
														<Plus className="w-4 h-4 mr-2" />
														{locale === "de" ? "Neue Sitzung" : "New Session"}
													</ContextMenuItem>
													<ContextMenuSeparator />
												</>
											)}
											<ContextMenuItem
												onClick={() => onPinProject(project.key)}
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
													onRenameProject(project.key, project.name)
												}
											>
												<Pencil className="w-4 h-4 mr-2" />
												{locale === "de" ? "Umbenennen" : "Rename"}
											</ContextMenuItem>
											<ContextMenuSeparator />
											<ContextMenuItem
												variant="destructive"
												onClick={() =>
													onDeleteProject(project.key, project.name)
												}
											>
												<Trash2 className="w-4 h-4 mr-2" />
												{locale === "de" ? "Loschen" : "Delete"} (
												{project.sessions.length}{" "}
												{project.sessions.length === 1 ? "chat" : "chats"})
											</ContextMenuItem>
										</ContextMenuContent>
									</ContextMenu>
									{/* Project sessions */}
									{isProjectExpanded && (
										<div className="space-y-0.5 pb-1">
											{project.sessions
												.filter((session) => !hiddenSessionIds.has(session.id))
												.map((session) => {
													const isSelected =
														selectedChatSessionId === session.id;
													const isMultiSelected = selectedSessionIds.has(
														session.id,
													);
													const children =
														sessionHierarchy.childSessionsByParent.get(
															session.id,
														) || [];
													const hasChildren = children.length > 0;
													const isExpanded = expandedSessions.has(session.id);
													const readableId = getReadableIdFromSession(session);
													const formattedDate = session.updated_at
														? formatSessionDate(session.updated_at)
														: null;
													return (
														<div
															key={session.id}
															className={isMobile ? "ml-4" : "ml-3"}
														>
															<ContextMenu>
																<ContextMenuTrigger className="contents">
																	<div
																		className={cn(
																			"w-full px-2 text-left transition-colors flex items-start gap-1.5 cursor-pointer",
																			isMobile ? "py-2" : "py-1",
																			isSelected
																				? "bg-primary/15 border border-primary text-foreground"
																				: isMultiSelected
																					? "bg-primary/10 border border-primary/50 text-foreground"
																					: "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
																		)}
																	>
																		{hasChildren ? (
																			<button
																				type="button"
																				onClick={() =>
																					toggleSessionExpanded(session.id)
																				}
																				onMouseDown={(e) => e.stopPropagation()}
																				className={cn(
																					"mt-0.5 hover:bg-muted flex-shrink-0 cursor-pointer",
																					isMobile ? "p-1" : "p-0.5",
																				)}
																			>
																				{isExpanded ? (
																					<ChevronDown
																						className={
																							isMobile ? "w-4 h-4" : "w-3 h-3"
																						}
																					/>
																				) : (
																					<ChevronRight
																						className={
																							isMobile ? "w-4 h-4" : "w-3 h-3"
																						}
																					/>
																				)}
																			</button>
																		) : (
																			<MessageSquare
																				className={cn(
																					"mt-0.5 flex-shrink-0 text-primary/70",
																					isMobile ? "w-4 h-4" : "w-3 h-3",
																				)}
																			/>
																		)}
																		<button
																			type="button"
																			onClick={(e) =>
																				handleSessionRowClick(e, session.id)
																			}
																			className="flex-1 min-w-0 text-left"
																		>
																			<div className="flex items-center gap-1">
																				{pinnedSessions.has(session.id) && (
																					<Pin className="w-3 h-3 flex-shrink-0 text-primary/70" />
																				)}
																				<span
																					className={cn(
																						"truncate font-medium",
																						sizeClasses.sessionText,
																					)}
																				>
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
																				<div
																					className={cn(
																						"text-muted-foreground mt-0.5",
																						sizeClasses.dateText,
																					)}
																				>
																					{formattedDate}
																				</div>
																			)}
																		</button>
																	</div>
																</ContextMenuTrigger>
																<ContextMenuContent>
																	{readableId && (
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
																	<ContextMenuItem
																		onClick={() => onPinSession(session.id)}
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
																		onClick={() => onRenameSession(session.id)}
																	>
																		<Pencil className="w-4 h-4 mr-2" />
																		{locale === "de" ? "Umbenennen" : "Rename"}
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
															{/* Child sessions (subagents) */}
															{hasChildren && isExpanded && (
																<div
																	className={cn(
																		"border-l border-muted pl-2 space-y-0.5 mt-0.5",
																		isMobile ? "ml-6 space-y-1 mt-1" : "ml-4",
																	)}
																>
																	{children
																		.filter(
																			(child) =>
																				!hiddenSessionIds.has(child.id),
																		)
																		.map((child) => {
																			const isChildSelected =
																				selectedChatSessionId === child.id;
																			const isChildMultiSelected =
																				selectedSessionIds.has(child.id);
																			const childReadableId =
																				getReadableIdFromSession(child);
																			const childFormattedDate =
																				child.updated_at
																					? formatSessionDate(child.updated_at)
																					: null;
																			return (
																				<ContextMenu key={child.id}>
																					<ContextMenuTrigger className="contents">
																						<button
																							type="button"
																							onClick={(e) =>
																								handleSessionRowClick(
																									e,
																									child.id,
																								)
																							}
																							className={cn(
																								"w-full px-2 text-left transition-colors",
																								isMobile
																									? "py-2 text-sm"
																									: "py-1 text-xs",
																								isChildSelected
																									? "bg-primary/15 border border-primary text-foreground"
																									: isChildMultiSelected
																										? "bg-primary/10 border border-primary/50 text-foreground"
																										: "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
																							)}
																						>
																							<div className="flex items-center gap-1">
																								<Bot
																									className={cn(
																										"flex-shrink-0 text-primary/70",
																										isMobile
																											? "w-3.5 h-3.5"
																											: "w-3 h-3",
																									)}
																								/>
																								<span className="truncate font-medium">
																									{child.title || "Subagent"}
																								</span>
																								{busySessions.has(child.id) && (
																									<Loader2 className="w-3 h-3 flex-shrink-0 text-primary animate-spin" />
																								)}
																							</div>
																							{childFormattedDate && (
																								<div
																									className={cn(
																										"text-muted-foreground mt-0.5",
																										isMobile
																											? "text-xs ml-5"
																											: "text-[9px] ml-4",
																									)}
																								>
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
																								onRenameSession(child.id)
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
																								handleDeleteSession(child.id)
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
	);
});
