import { MainChatEntry } from "@/components/main-chat";
import {
	type AgentFilter,
	type SearchMode,
	SearchResults,
} from "@/components/search";
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
import { formatSessionDate, resolveReadableId } from "@/lib/session-utils";
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
	Loader2,
	MessageSquare,
	Pencil,
	Pin,
	Plus,
	Search,
	Trash2,
	X,
} from "lucide-react";
import { memo, useDeferredValue, useEffect, useState } from "react";

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
	mainChatActive: boolean;
	mainChatCurrentSessionId: string | null;
	mainChatNewSessionTrigger: number;
	mainChatSessionActivityTrigger: number;
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
	onPinProject: (projectKey: string) => void;
	onRenameProject: (projectKey: string, currentName: string) => void;
	onDeleteProject: (projectKey: string, projectName: string) => void;
	onMainChatSelect: () => void;
	onMainChatSessionSelect: (sessionId: string) => void;
	onMainChatNewSession: () => void;
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
	mainChatActive,
	mainChatCurrentSessionId,
	mainChatNewSessionTrigger,
	mainChatSessionActivityTrigger,
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
	onPinProject,
	onRenameProject,
	onDeleteProject,
	onMainChatSelect,
	onMainChatSessionSelect,
	onMainChatNewSession,
	onSearchResultClick,
	messageSearchExtraHits,
	isMobile = false,
}: SidebarSessionsProps) {
	// Session search
	const [sessionSearch, setSessionSearch] = useState("");
	const deferredSearch = useDeferredValue(sessionSearch);
	const [searchMode, setSearchMode] = useState<SearchMode>("sessions");
	const [agentFilter, setAgentFilter] = useState<AgentFilter>("all");
	const [mainChatFilterCount, setMainChatFilterCount] = useState(0);
	const [mainChatTotalCount, setMainChatTotalCount] = useState(0);
	const isFilteringSessions =
		searchMode === "sessions" && deferredSearch.trim().length > 0;

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

	return (
		<div className="flex-1 min-h-0 flex flex-col overflow-x-hidden">
			{/* Sticky header section - Search, Main Chat, Sessions header */}
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
										onClick={() => setAgentFilter("opencode")}
										className={cn(agentFilter === "opencode" && "bg-accent")}
									>
										{locale === "de" ? "Nur OpenCode" : "OpenCode only"}
									</DropdownMenuItem>
									<DropdownMenuItem
										onClick={() => setAgentFilter("pi_agent")}
										className={cn(agentFilter === "pi_agent" && "bg-accent")}
									>
										{locale === "de" ? "Nur Main Chat" : "Main Chat only"}
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
							(
							{isFilteringSessions
								? filteredSessions.length + mainChatFilterCount
								: filteredSessions.length}
							{deferredSearch
								? `/${chatHistory.length + mainChatTotalCount}`
								: ""}
							)
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
						{/* Main Chat - shown at top of sessions list */}
						<MainChatEntry
							isSelected={mainChatActive}
							activeSessionId={mainChatActive ? mainChatCurrentSessionId : null}
							newSessionTrigger={mainChatNewSessionTrigger}
							sessionActivityTrigger={mainChatSessionActivityTrigger}
							onSelect={onMainChatSelect}
							onSessionSelect={onMainChatSessionSelect}
							onNewSession={onMainChatNewSession}
							locale={locale}
							filterQuery={searchMode === "sessions" ? deferredSearch : ""}
							onFilterCountChange={setMainChatFilterCount}
							onTotalCountChange={setMainChatTotalCount}
						/>
						{filteredSessions.length === 0 &&
							deferredSearch &&
							mainChatFilterCount === 0 && (
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
											{project.sessions.map((session) => {
												const isSelected = selectedChatSessionId === session.id;
												const children =
													sessionHierarchy.childSessionsByParent.get(
														session.id,
													) || [];
												const hasChildren = children.length > 0;
												const isExpanded = expandedSessions.has(session.id);
												const readableId = resolveReadableId(
													session.id,
													session.readable_id,
												);
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
																			: "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
																	)}
																>
																	{hasChildren ? (
																		<button
																			type="button"
																			onClick={() =>
																				toggleSessionExpanded(session.id)
																			}
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
																		onClick={() => onSessionClick(session.id)}
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
																<ContextMenuItem
																	onClick={() => {
																		navigator.clipboard.writeText(readableId);
																	}}
																>
																	<Copy className="w-4 h-4 mr-2" />
																	{readableId}
																</ContextMenuItem>
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
																	onClick={() => onDeleteSession(session.id)}
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
																{children.map((child) => {
																	const isChildSelected =
																		selectedChatSessionId === child.id;
																	const childReadableId = resolveReadableId(
																		child.id,
																		child.readable_id,
																	);
																	const childFormattedDate = child.updated_at
																		? formatSessionDate(child.updated_at)
																		: null;
																	return (
																		<ContextMenu key={child.id}>
																			<ContextMenuTrigger className="contents">
																				<button
																					type="button"
																					onClick={() =>
																						onSessionClick(child.id)
																					}
																					className={cn(
																						"w-full px-2 text-left transition-colors",
																						isMobile
																							? "py-2 text-sm"
																							: "py-1 text-xs",
																						isChildSelected
																							? "bg-primary/15 border border-primary text-foreground"
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
																						onDeleteSession(child.id)
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
