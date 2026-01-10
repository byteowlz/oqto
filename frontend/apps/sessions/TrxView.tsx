"use client";

import { Badge } from "@/components/ui/badge";
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
	DropdownMenuCheckboxItem,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { controlPlaneApiUrl, getAuthHeaders } from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import {
	AlertCircle,
	ArrowDownAZ,
	ArrowUpDown,
	Bug,
	Check,
	CheckCircle2,
	ChevronDown,
	ChevronRight,
	ChevronUp,
	CircleDot,
	ClipboardList,
	ExternalLink,
	Filter,
	Loader2,
	Mountain,
	Package,
	Pause,
	Pencil,
	Play,
	Plus,
	RefreshCw,
	Search,
	Trash2,
	X,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useState } from "react";

// TRX issue structure from backend
interface TrxIssue {
	id: string;
	title: string;
	description?: string;
	status: string;
	priority: number;
	issue_type: string;
	created_at: string;
	updated_at: string;
	closed_at?: string;
	parent_id?: string;
	labels: string[];
	blocked_by: string[];
}

interface TrxViewProps {
	workspacePath?: string;
	className?: string;
	onStartIssue?: (issueId: string, title: string, description?: string) => void;
	onStartIssueNewSession?: (
		issueId: string,
		title: string,
		description?: string,
	) => void;
}

// API functions
async function fetchTrxIssues(workspacePath: string): Promise<TrxIssue[]> {
	const url = new URL(
		controlPlaneApiUrl("/api/workspace/trx/issues"),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);

	const res = await fetch(url.toString(), {
		credentials: "include",
		headers: getAuthHeaders(),
	});
	if (!res.ok) {
		if (res.status === 404) {
			// No .trx directory - not initialized
			return [];
		}
		throw new Error(`Failed to fetch TRX issues: ${res.statusText}`);
	}
	return res.json();
}

async function createTrxIssue(
	workspacePath: string,
	data: {
		title: string;
		description?: string;
		issue_type?: string;
		priority?: number;
		parent_id?: string;
	},
): Promise<TrxIssue> {
	const url = new URL(
		controlPlaneApiUrl("/api/workspace/trx/issues"),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);

	const res = await fetch(url.toString(), {
		method: "POST",
		credentials: "include",
		headers: { "Content-Type": "application/json", ...getAuthHeaders() },
		body: JSON.stringify(data),
	});
	if (!res.ok) {
		const text = await res.text();
		throw new Error(`Failed to create issue: ${text || res.statusText}`);
	}
	return res.json();
}

async function updateTrxIssue(
	workspacePath: string,
	issueId: string,
	data: {
		title?: string;
		description?: string;
		status?: string;
		priority?: number;
	},
): Promise<TrxIssue> {
	const url = new URL(
		controlPlaneApiUrl(`/api/workspace/trx/issues/${issueId}`),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);

	const res = await fetch(url.toString(), {
		method: "PUT",
		credentials: "include",
		headers: { "Content-Type": "application/json", ...getAuthHeaders() },
		body: JSON.stringify(data),
	});
	if (!res.ok) {
		throw new Error(`Failed to update issue: ${res.statusText}`);
	}
	return res.json();
}

async function closeTrxIssue(
	workspacePath: string,
	issueId: string,
	reason?: string,
): Promise<TrxIssue> {
	const url = new URL(
		controlPlaneApiUrl(`/api/workspace/trx/issues/${issueId}/close`),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);

	const res = await fetch(url.toString(), {
		method: "POST",
		credentials: "include",
		headers: { "Content-Type": "application/json", ...getAuthHeaders() },
		body: JSON.stringify({ reason }),
	});
	if (!res.ok) {
		throw new Error(`Failed to close issue: ${res.statusText}`);
	}
	return res.json();
}

// Issue type icons and colors
const issueTypeConfig: Record<
	string,
	{ icon: typeof Bug; color: string; label: string }
> = {
	bug: { icon: Bug, color: "text-red-400", label: "Bug" },
	feature: { icon: Package, color: "text-purple-400", label: "Feature" },
	task: { icon: ClipboardList, color: "text-blue-400", label: "Task" },
	epic: { icon: Mountain, color: "text-amber-400", label: "Epic" },
	chore: { icon: CircleDot, color: "text-gray-400", label: "Chore" },
};

// Priority colors
const priorityColors: Record<number, string> = {
	0: "bg-red-500/20 text-red-400 border-red-500/30",
	1: "bg-orange-500/20 text-orange-400 border-orange-500/30",
	2: "bg-yellow-500/20 text-yellow-400 border-yellow-500/30",
	3: "bg-green-500/20 text-green-400 border-green-500/30",
	4: "bg-gray-500/20 text-gray-400 border-gray-500/30",
};

// Status colors
const statusColors: Record<string, string> = {
	open: "bg-blue-500/20 text-blue-400",
	in_progress: "bg-purple-500/20 text-purple-400",
	closed: "bg-green-500/20 text-green-400",
	blocked: "bg-red-500/20 text-red-400",
};

// Issue card component
const IssueCard = memo(function IssueCard({
	issue,
	childIssues,
	childrenByParent,
	expandedIssues,
	onToggleExpand,
	isExpanded,
	onToggle,
	onStatusChange,
	onStartHere,
	onStartNewSession,
	onAddChild,
	onEdit,
	isEditing,
	editTitle,
	onEditTitleChange,
	onEditSave,
	onEditCancel,
	depth = 0,
}: {
	issue: TrxIssue;
	childIssues?: TrxIssue[];
	childrenByParent?: Map<string, TrxIssue[]>;
	expandedIssues?: Set<string>;
	onToggleExpand?: (issueId: string) => void;
	isExpanded: boolean;
	onToggle: () => void;
	onStatusChange: (status: string) => void;
	onStartHere?: () => void;
	onStartNewSession?: () => void;
	onAddChild: () => void;
	onEdit: () => void;
	isEditing?: boolean;
	editTitle?: string;
	onEditTitleChange?: (title: string) => void;
	onEditSave?: () => void;
	onEditCancel?: () => void;
	depth?: number;
}) {
	const typeConfig = issueTypeConfig[issue.issue_type] || issueTypeConfig.task;
	const TypeIcon = typeConfig.icon;
	const hasChildren = childIssues && childIssues.length > 0;
	const isClosed = issue.status === "closed";

	return (
		<div className={cn("space-y-1", depth > 0 && "ml-6")}>
			<ContextMenu>
				<ContextMenuTrigger className="contents">
					<div
						className={cn(
							"group p-2 rounded transition-colors cursor-context-menu flex gap-2",
							isClosed ? "opacity-50" : "hover:bg-muted/50",
							isEditing && "bg-muted/50 ring-1 ring-primary/50",
						)}
					>
						{/* Left column: Type icon + Chevron below (aligned with row 2 or 3) */}
						<div className="flex flex-col items-center flex-shrink-0 pt-0.5 w-4">
							<TypeIcon className={cn("w-4 h-4", typeConfig.color)} />
							{hasChildren && (
								<button
									type="button"
									onClick={onToggle}
									className={cn(
										"p-0.5 hover:bg-muted rounded",
										issue.description ? "mt-1" : "mt-2.5",
									)}
								>
									{isExpanded ? (
										<ChevronDown className="w-3 h-3 text-muted-foreground" />
									) : (
										<ChevronRight className="w-3 h-3 text-muted-foreground" />
									)}
								</button>
							)}
						</div>

						{/* Right column: Content rows */}
						<div className="flex-1 min-w-0">
							{/* Row 1: Title + ID */}
							{isEditing ? (
								<div className="flex items-center gap-1">
									<Input
										value={editTitle}
										onChange={(e) => onEditTitleChange?.(e.target.value)}
										onKeyDown={(e) => {
											if (e.key === "Enter") onEditSave?.();
											if (e.key === "Escape") onEditCancel?.();
										}}
										className="h-6 text-sm py-0"
										autoFocus
									/>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={onEditSave}
										className="h-6 w-6 p-0"
									>
										<Check className="w-3 h-3 text-green-500" />
									</Button>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={onEditCancel}
										className="h-6 w-6 p-0"
									>
										<X className="w-3 h-3" />
									</Button>
								</div>
							) : (
								<div className="flex items-center gap-2">
									<Tooltip>
										<TooltipTrigger asChild>
											<span
												className={cn(
													"text-sm font-medium truncate cursor-default flex-1",
													isClosed && "line-through text-muted-foreground",
												)}
											>
												{issue.title}
											</span>
										</TooltipTrigger>
										<TooltipContent
											side="top"
											className="max-w-xs bg-popover text-popover-foreground border border-border shadow-md p-2"
										>
											<p className="font-medium text-sm">{issue.title}</p>
											{issue.description && (
												<p className="text-xs text-muted-foreground mt-1 line-clamp-3">
													{issue.description}
												</p>
											)}
										</TooltipContent>
									</Tooltip>
									<span className="text-[10px] font-mono text-muted-foreground flex-shrink-0">
										{issue.id}
									</span>
								</div>
							)}

							{/* Row 2: Description */}
							{!isEditing && issue.description && (
								<p className="text-[11px] text-muted-foreground truncate mt-0.5">
									{issue.description}
								</p>
							)}

							{/* Row 3: Status, Priority, Actions */}
							<div className="flex items-center gap-1 mt-1">
								<Badge
									variant="outline"
									className={cn(
										"text-[9px] px-1 py-0 h-4",
										statusColors[issue.status] || statusColors.open,
									)}
								>
									{issue.status.replace("_", " ")}
								</Badge>
								<Badge
									variant="outline"
									className={cn(
										"text-[9px] px-1 py-0 h-4 border",
										priorityColors[issue.priority] || priorityColors[2],
									)}
								>
									P{issue.priority}
								</Badge>

								<div className="flex-1" />

								{/* Actions */}
								{!isClosed && (
									<div className="flex items-center gap-0.5">
										{issue.status !== "in_progress" &&
											(onStartHere || onStartNewSession) && (
												<DropdownMenu>
													<DropdownMenuTrigger asChild>
														<Button
															type="button"
															variant="ghost"
															size="sm"
															onClick={(e) => e.stopPropagation()}
															className="h-5 w-5 p-0"
															title="Start working"
														>
															<Play className="w-3 h-3 text-muted-foreground" />
														</Button>
													</DropdownMenuTrigger>
													<DropdownMenuContent align="end" className="w-40">
														{onStartHere && (
															<DropdownMenuItem
																onClick={onStartHere}
																className="text-xs"
															>
																<Play className="w-3 h-3 mr-2" />
																Start here
															</DropdownMenuItem>
														)}
														{onStartNewSession && (
															<DropdownMenuItem
																onClick={onStartNewSession}
																className="text-xs"
															>
																<ExternalLink className="w-3 h-3 mr-2" />
																Start in new session
															</DropdownMenuItem>
														)}
													</DropdownMenuContent>
												</DropdownMenu>
											)}
										{issue.status === "in_progress" && (
											<Button
												type="button"
												variant="ghost"
												size="sm"
												onClick={(e) => {
													e.stopPropagation();
													onStatusChange("open");
												}}
												className="h-5 w-5 p-0"
												title="Pause"
											>
												<Pause className="w-3 h-3 text-muted-foreground" />
											</Button>
										)}
										<Button
											type="button"
											variant="ghost"
											size="sm"
											onClick={(e) => {
												e.stopPropagation();
												onStatusChange("closed");
											}}
											className="h-5 w-5 p-0"
											title="Mark as done"
										>
											<CheckCircle2 className="w-3 h-3 text-muted-foreground" />
										</Button>
									</div>
								)}
							</div>
						</div>
					</div>
				</ContextMenuTrigger>
				<ContextMenuContent>
					{!isClosed && (
						<>
							<ContextMenuItem onClick={onAddChild}>
								<Plus className="w-4 h-4" />
								Add child issue
							</ContextMenuItem>
							<ContextMenuSeparator />
							{issue.status !== "in_progress" && onStartHere && (
								<ContextMenuItem onClick={onStartHere}>
									<Play className="w-4 h-4" />
									Start here
								</ContextMenuItem>
							)}
							{issue.status !== "in_progress" && onStartNewSession && (
								<ContextMenuItem onClick={onStartNewSession}>
									<ExternalLink className="w-4 h-4" />
									Start in new session
								</ContextMenuItem>
							)}
							{issue.status === "in_progress" && (
								<ContextMenuItem onClick={() => onStatusChange("open")}>
									<Pause className="w-4 h-4" />
									Pause
								</ContextMenuItem>
							)}
							<ContextMenuItem onClick={() => onStatusChange("closed")}>
								<CheckCircle2 className="w-4 h-4" />
								Mark as done
							</ContextMenuItem>
							<ContextMenuSeparator />
							<ContextMenuItem onClick={onEdit}>
								<Pencil className="w-4 h-4" />
								Edit
							</ContextMenuItem>
						</>
					)}
					{isClosed && (
						<ContextMenuItem onClick={() => onStatusChange("open")}>
							<RefreshCw className="w-4 h-4" />
							Reopen issue
						</ContextMenuItem>
					)}
				</ContextMenuContent>
			</ContextMenu>

			{/* Children (recursive for multi-level nesting) */}
			{hasChildren && isExpanded && (
				<div className="space-y-1">
					{childIssues.map((child) => {
						const grandchildren = childrenByParent?.get(child.id);
						const childIsExpanded = expandedIssues?.has(child.id) ?? false;
						return (
							<IssueCard
								key={child.id}
								issue={child}
								childIssues={grandchildren}
								childrenByParent={childrenByParent}
								expandedIssues={expandedIssues}
								onToggleExpand={onToggleExpand}
								isExpanded={childIsExpanded}
								onToggle={() => onToggleExpand?.(child.id)}
								onStatusChange={(status) => onStatusChange(status)}
								onStartHere={onStartHere}
								onStartNewSession={onStartNewSession}
								onAddChild={onAddChild}
								onEdit={onEdit}
								depth={depth + 1}
							/>
						);
					})}
				</div>
			)}
		</div>
	);
});

export const TrxView = memo(function TrxView({
	workspacePath,
	className,
	onStartIssue,
	onStartIssueNewSession,
}: TrxViewProps) {
	const [issues, setIssues] = useState<TrxIssue[]>([]);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string>("");
	const [expandedEpics, setExpandedEpics] = useState<Set<string>>(new Set());
	const [showAddForm, setShowAddForm] = useState(false);
	const [newIssueTitle, setNewIssueTitle] = useState("");
	const [newIssueType, setNewIssueType] = useState("task");
	const [newIssueParentId, setNewIssueParentId] = useState<string | null>(null);
	const [isCreating, setIsCreating] = useState(false);
	const [isCollapsed, setIsCollapsed] = useState(false);

	// Edit state
	const [editingIssueId, setEditingIssueId] = useState<string | null>(null);
	const [editTitle, setEditTitle] = useState("");

	// Sort and filter state
	type SortOption = "status" | "priority" | "created" | "updated";
	type FilterStatus = "all" | "open" | "in_progress" | "closed";
	type FilterType = "all" | "bug" | "feature" | "task" | "epic" | "chore";
	const [sortBy, setSortBy] = useState<SortOption>("status");
	const [filterStatus, setFilterStatus] = useState<FilterStatus>("all");
	const [filterType, setFilterType] = useState<FilterType>("all");
	const [hideClosed, setHideClosed] = useState(false);
	const [searchQuery, setSearchQuery] = useState("");
	const [searchIncludeDescription, setSearchIncludeDescription] =
		useState(false);

	const loadIssues = useCallback(async () => {
		if (!workspacePath) {
			setLoading(false);
			return;
		}

		setLoading(true);
		setError("");
		try {
			const data = await fetchTrxIssues(workspacePath);
			setIssues(data);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to load issues");
		} finally {
			setLoading(false);
		}
	}, [workspacePath]);

	// Load issues when workspace path changes
	useEffect(() => {
		// Reset state and load when workspace changes
		if (workspacePath) {
			setIssues([]);
			setError("");
			loadIssues();
		}
	}, [workspacePath, loadIssues]);

	// Sort function
	const sortIssues = useCallback(
		(issueList: TrxIssue[]): TrxIssue[] => {
			return [...issueList].sort((a, b) => {
				// Status priority: in_progress > open > blocked > closed
				const statusOrder: Record<string, number> = {
					in_progress: 0,
					open: 1,
					blocked: 2,
					closed: 3,
				};

				switch (sortBy) {
					case "status":
						return (
							(statusOrder[a.status] ?? 99) - (statusOrder[b.status] ?? 99)
						);
					case "priority":
						return a.priority - b.priority;
					case "created":
						return (
							new Date(b.created_at).getTime() -
							new Date(a.created_at).getTime()
						);
					case "updated":
						return (
							new Date(b.updated_at).getTime() -
							new Date(a.updated_at).getTime()
						);
					default:
						return 0;
				}
			});
		},
		[sortBy],
	);

	// Fuzzy match function
	const fuzzyMatch = useCallback((text: string, query: string): boolean => {
		if (!query) return true;
		const lowerText = text.toLowerCase();
		const lowerQuery = query.toLowerCase();

		// Check for direct substring match first
		if (lowerText.includes(lowerQuery)) return true;

		// Fuzzy match: all query chars must appear in order
		let queryIndex = 0;
		for (const char of lowerText) {
			if (char === lowerQuery[queryIndex]) {
				queryIndex++;
				if (queryIndex === lowerQuery.length) return true;
			}
		}
		return false;
	}, []);

	// Filter function
	const filterIssues = useCallback(
		(issueList: TrxIssue[]): TrxIssue[] => {
			return issueList.filter((issue) => {
				if (hideClosed && issue.status === "closed") return false;
				if (filterStatus !== "all" && issue.status !== filterStatus)
					return false;
				if (filterType !== "all" && issue.issue_type !== filterType)
					return false;
				// Fuzzy search on title, optionally description, and id
				if (searchQuery) {
					const matchesTitle = fuzzyMatch(issue.title, searchQuery);
					const matchesDescription =
						searchIncludeDescription && issue.description
							? fuzzyMatch(issue.description, searchQuery)
							: false;
					const matchesId = fuzzyMatch(issue.id, searchQuery);
					if (!matchesTitle && !matchesDescription && !matchesId) return false;
				}
				return true;
			});
		},
		[
			filterStatus,
			filterType,
			hideClosed,
			searchQuery,
			searchIncludeDescription,
			fuzzyMatch,
		],
	);

	// Organize issues into hierarchy (parents with children)
	const { parentIssues, standaloneIssues, childrenByParent } = useMemo(() => {
		const childrenByParent = new Map<string, TrxIssue[]>();
		const issueById = new Map<string, TrxIssue>();

		// Apply filtering first
		const filteredIssues = filterIssues(issues);

		// First pass: index all issues and build parent-child map
		for (const issue of filteredIssues) {
			issueById.set(issue.id, issue);
			if (issue.parent_id) {
				const existing = childrenByParent.get(issue.parent_id) || [];
				existing.push(issue);
				childrenByParent.set(issue.parent_id, existing);
			}
		}

		// Second pass: separate parents from standalone
		const parentIssues: TrxIssue[] = [];
		const standaloneIssues: TrxIssue[] = [];

		for (const issue of filteredIssues) {
			if (issue.parent_id) {
				// This is a child, skip (will be shown under parent)
				continue;
			}
			if (childrenByParent.has(issue.id)) {
				// This issue has children, treat as parent
				parentIssues.push(issue);
			} else {
				standaloneIssues.push(issue);
			}
		}

		// Sort children within each parent
		for (const [parentId, children] of childrenByParent) {
			childrenByParent.set(parentId, sortIssues(children));
		}

		return {
			parentIssues: sortIssues(parentIssues),
			standaloneIssues: sortIssues(standaloneIssues),
			childrenByParent,
		};
	}, [issues, sortIssues, filterIssues]);

	const handleToggleEpic = useCallback((epicId: string) => {
		setExpandedEpics((prev) => {
			const next = new Set(prev);
			if (next.has(epicId)) {
				next.delete(epicId);
			} else {
				next.add(epicId);
			}
			return next;
		});
	}, []);

	const handleStatusChange = useCallback(
		async (issueId: string, status: string) => {
			if (!workspacePath) return;

			try {
				if (status === "closed") {
					await closeTrxIssue(workspacePath, issueId);
				} else {
					await updateTrxIssue(workspacePath, issueId, { status });
				}
				await loadIssues();
			} catch (err) {
				setError(err instanceof Error ? err.message : "Failed to update issue");
			}
		},
		[workspacePath, loadIssues],
	);

	const handleStartIssue = useCallback(
		async (issue: TrxIssue) => {
			if (!workspacePath) return;

			try {
				// Set status to in_progress
				await updateTrxIssue(workspacePath, issue.id, {
					status: "in_progress",
				});
				await loadIssues();

				// Call the callback to prefill input and switch view
				onStartIssue?.(issue.id, issue.title, issue.description);
			} catch (err) {
				setError(err instanceof Error ? err.message : "Failed to start issue");
			}
		},
		[workspacePath, loadIssues, onStartIssue],
	);

	const handleStartIssueNewSession = useCallback(
		async (issue: TrxIssue) => {
			if (!workspacePath) return;

			try {
				// Set status to in_progress
				await updateTrxIssue(workspacePath, issue.id, {
					status: "in_progress",
				});
				await loadIssues();

				// Call the callback to open new session with issue
				onStartIssueNewSession?.(issue.id, issue.title, issue.description);
			} catch (err) {
				setError(err instanceof Error ? err.message : "Failed to start issue");
			}
		},
		[workspacePath, loadIssues, onStartIssueNewSession],
	);

	const handleCreate = useCallback(async () => {
		if (!workspacePath || !newIssueTitle.trim()) return;

		setIsCreating(true);
		setError("");
		try {
			await createTrxIssue(workspacePath, {
				title: newIssueTitle,
				issue_type: newIssueType,
				parent_id: newIssueParentId ?? undefined,
			});
			setNewIssueTitle("");
			setNewIssueParentId(null);
			setShowAddForm(false);
			await loadIssues();
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to create issue");
		} finally {
			setIsCreating(false);
		}
	}, [
		workspacePath,
		newIssueTitle,
		newIssueType,
		newIssueParentId,
		loadIssues,
	]);

	const handleAddChild = useCallback((parentId: string) => {
		setNewIssueParentId(parentId);
		setShowAddForm(true);
	}, []);

	const handleStartEdit = useCallback((issue: TrxIssue) => {
		setEditingIssueId(issue.id);
		setEditTitle(issue.title);
	}, []);

	const handleSaveEdit = useCallback(async () => {
		if (!workspacePath || !editingIssueId || !editTitle.trim()) return;

		try {
			await updateTrxIssue(workspacePath, editingIssueId, {
				title: editTitle.trim(),
			});
			setEditingIssueId(null);
			setEditTitle("");
			await loadIssues();
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to update issue");
		}
	}, [workspacePath, editingIssueId, editTitle, loadIssues]);

	const handleCancelEdit = useCallback(() => {
		setEditingIssueId(null);
		setEditTitle("");
	}, []);

	// Summary stats
	const stats = useMemo(() => {
		const open = issues.filter((i) => i.status === "open").length;
		const inProgress = issues.filter((i) => i.status === "in_progress").length;
		const closed = issues.filter((i) => i.status === "closed").length;
		return { open, inProgress, closed, total: issues.length };
	}, [issues]);

	if (!workspacePath) {
		return (
			<div className={cn("flex items-center justify-center h-full", className)}>
				<div className="text-center text-muted-foreground">
					<ClipboardList className="w-8 h-8 mx-auto mb-2 opacity-50" />
					<p className="text-xs">No workspace selected</p>
				</div>
			</div>
		);
	}

	if (loading) {
		return (
			<div className={cn("flex items-center justify-center h-full", className)}>
				<div className="text-center text-muted-foreground">
					<Loader2 className="w-6 h-6 mx-auto mb-2 animate-spin" />
					<p className="text-xs">Loading issues...</p>
				</div>
			</div>
		);
	}

	// Collapsed view - just a status bar
	if (isCollapsed) {
		return (
			<div className={cn("flex-shrink-0", className)}>
				<button
					type="button"
					onClick={() => setIsCollapsed(false)}
					className="w-full flex items-center justify-between px-3 py-2 bg-muted/30 hover:bg-muted/50 transition-colors"
				>
					<div className="flex items-center gap-3 text-[11px] text-muted-foreground">
						<ClipboardList className="w-3.5 h-3.5" />
						<span className="font-medium">Issues</span>
						{stats.total > 0 && (
							<>
								<span>{stats.total} total</span>
								{stats.inProgress > 0 && (
									<span className="text-purple-400">
										{stats.inProgress} active
									</span>
								)}
								{stats.open > 0 && (
									<span className="text-blue-400">{stats.open} open</span>
								)}
							</>
						)}
					</div>
					<ChevronUp className="w-3.5 h-3.5 text-muted-foreground" />
				</button>
			</div>
		);
	}

	return (
		<div className={cn("flex flex-col h-full overflow-hidden", className)}>
			{/* Header */}
			<div className="flex-shrink-0 px-3 py-2 border-b border-border">
				<div className="flex items-center justify-between mb-2">
					<span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						Issues
					</span>
					<div className="flex items-center gap-1">
						{/* Sort/Filter dropdown */}
						<DropdownMenu>
							<DropdownMenuTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="sm"
									className="h-6 w-6 p-0"
									title="Sort & Filter"
								>
									<ArrowUpDown className="w-3 h-3" />
								</Button>
							</DropdownMenuTrigger>
							<DropdownMenuContent align="end" className="w-48">
								<DropdownMenuLabel className="text-xs">
									Sort by
								</DropdownMenuLabel>
								<DropdownMenuCheckboxItem
									checked={sortBy === "status"}
									onCheckedChange={() => setSortBy("status")}
									className="text-xs"
								>
									Status (active first)
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={sortBy === "priority"}
									onCheckedChange={() => setSortBy("priority")}
									className="text-xs"
								>
									Priority
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={sortBy === "created"}
									onCheckedChange={() => setSortBy("created")}
									className="text-xs"
								>
									Recently created
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={sortBy === "updated"}
									onCheckedChange={() => setSortBy("updated")}
									className="text-xs"
								>
									Recently updated
								</DropdownMenuCheckboxItem>
								<DropdownMenuSeparator />
								<DropdownMenuLabel className="text-xs">
									Filter
								</DropdownMenuLabel>
								<DropdownMenuCheckboxItem
									checked={filterStatus === "all"}
									onCheckedChange={() => setFilterStatus("all")}
									className="text-xs"
								>
									All statuses
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={filterStatus === "in_progress"}
									onCheckedChange={() => setFilterStatus("in_progress")}
									className="text-xs"
								>
									In progress only
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={filterStatus === "open"}
									onCheckedChange={() => setFilterStatus("open")}
									className="text-xs"
								>
									Open only
								</DropdownMenuCheckboxItem>
								<DropdownMenuSeparator />
								<DropdownMenuLabel className="text-xs">Type</DropdownMenuLabel>
								<DropdownMenuCheckboxItem
									checked={filterType === "all"}
									onCheckedChange={() => setFilterType("all")}
									className="text-xs"
								>
									All types
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={filterType === "bug"}
									onCheckedChange={() => setFilterType("bug")}
									className="text-xs"
								>
									Bugs
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={filterType === "feature"}
									onCheckedChange={() => setFilterType("feature")}
									className="text-xs"
								>
									Features
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={filterType === "task"}
									onCheckedChange={() => setFilterType("task")}
									className="text-xs"
								>
									Tasks
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={filterType === "epic"}
									onCheckedChange={() => setFilterType("epic")}
									className="text-xs"
								>
									Epics
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={filterType === "chore"}
									onCheckedChange={() => setFilterType("chore")}
									className="text-xs"
								>
									Chores
								</DropdownMenuCheckboxItem>
								<DropdownMenuSeparator />
								<DropdownMenuCheckboxItem
									checked={hideClosed}
									onCheckedChange={setHideClosed}
									className="text-xs"
								>
									Hide closed
								</DropdownMenuCheckboxItem>
								<DropdownMenuCheckboxItem
									checked={searchIncludeDescription}
									onCheckedChange={setSearchIncludeDescription}
									className="text-xs"
								>
									Search in description
								</DropdownMenuCheckboxItem>
							</DropdownMenuContent>
						</DropdownMenu>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={loadIssues}
							disabled={loading}
							className="h-6 w-6 p-0"
							title="Refresh"
						>
							<RefreshCw className={cn("w-3 h-3", loading && "animate-spin")} />
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => {
								setNewIssueParentId(null);
								setShowAddForm(!showAddForm);
							}}
							className="h-6 w-6 p-0"
							title="Add issue"
						>
							<Plus className="w-3 h-3" />
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => setIsCollapsed(true)}
							className="h-6 w-6 p-0"
							title="Collapse"
						>
							<ChevronDown className="w-3 h-3" />
						</Button>
					</div>
				</div>

				{/* Search bar */}
				<div className="relative mt-2">
					<Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3 h-3 text-muted-foreground" />
					<Input
						value={searchQuery}
						onChange={(e) => setSearchQuery(e.target.value)}
						placeholder="Search issues..."
						className="h-7 text-xs pl-7 pr-7 shadow-none border-none bg-muted/30 focus-visible:ring-1 focus-visible:ring-border"
					/>
					{searchQuery && (
						<button
							type="button"
							onClick={() => setSearchQuery("")}
							className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
						>
							<X className="w-3 h-3" />
						</button>
					)}
				</div>

				{/* Stats bar */}
				{stats.total > 0 && (
					<div className="flex items-center gap-3 text-[10px] text-muted-foreground mt-2">
						<span>{stats.total} total</span>
						{stats.inProgress > 0 && (
							<span className="text-purple-400">{stats.inProgress} active</span>
						)}
						{stats.open > 0 && (
							<span className="text-blue-400">{stats.open} open</span>
						)}
						{stats.closed > 0 && (
							<span className="text-green-400">{stats.closed} done</span>
						)}
					</div>
				)}

				{/* Add form */}
				{showAddForm && (
					<div className="mt-2 p-2 bg-muted/30 rounded space-y-2">
						{newIssueParentId && (
							<div className="flex items-center gap-2 text-[10px] text-muted-foreground">
								<span>Adding child to:</span>
								<span className="font-mono bg-muted px-1 rounded">
									{newIssueParentId}
								</span>
								<button
									type="button"
									onClick={() => setNewIssueParentId(null)}
									className="text-muted-foreground hover:text-foreground"
								>
									<X className="w-3 h-3" />
								</button>
							</div>
						)}
						<Input
							value={newIssueTitle}
							onChange={(e) => setNewIssueTitle(e.target.value)}
							placeholder={
								newIssueParentId ? "Child issue title..." : "Issue title..."
							}
							className="h-7 text-xs"
							onKeyDown={(e) => e.key === "Enter" && handleCreate()}
							autoFocus
						/>
						<div className="flex items-center gap-2">
							<select
								value={newIssueType}
								onChange={(e) => setNewIssueType(e.target.value)}
								className="h-6 text-xs bg-background border border-border rounded px-2"
							>
								<option value="task">Task</option>
								<option value="bug">Bug</option>
								<option value="feature">Feature</option>
								<option value="epic">Epic</option>
								<option value="chore">Chore</option>
							</select>
							<div className="flex-1" />
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => {
									setShowAddForm(false);
									setNewIssueParentId(null);
								}}
								className="h-6 px-2 text-xs"
							>
								Cancel
							</Button>
							<Button
								type="button"
								variant="default"
								size="sm"
								onClick={handleCreate}
								disabled={isCreating || !newIssueTitle.trim()}
								className="h-6 px-2 text-xs"
							>
								{isCreating ? (
									<Loader2 className="w-3 h-3 animate-spin" />
								) : (
									"Create"
								)}
							</Button>
						</div>
					</div>
				)}
			</div>

			{/* Error message */}
			{error && (
				<div className="flex-shrink-0 px-2 py-1 bg-destructive/10 text-destructive text-xs flex items-center gap-1">
					<AlertCircle className="w-3 h-3" />
					{error}
				</div>
			)}

			{/* Issues list */}
			<div className="flex-1 overflow-auto px-3 py-2 space-y-1">
				{issues.length === 0 ? (
					<div className="flex items-center justify-center h-full">
						<div className="text-center text-muted-foreground">
							<ClipboardList className="w-8 h-8 mx-auto mb-2 opacity-50" />
							<p className="text-xs">No issues yet</p>
							<p className="text-[10px] mt-1">
								Click + to create one or run{" "}
								<code className="bg-muted px-1 rounded">trx init</code>
							</p>
						</div>
					</div>
				) : (
					<>
						{/* Parent issues (epics, features, etc. with children) first */}
						{parentIssues.map((parent) => (
							<IssueCard
								key={parent.id}
								issue={parent}
								childIssues={childrenByParent.get(parent.id)}
								childrenByParent={childrenByParent}
								expandedIssues={expandedEpics}
								onToggleExpand={handleToggleEpic}
								isExpanded={expandedEpics.has(parent.id)}
								onToggle={() => handleToggleEpic(parent.id)}
								onStatusChange={(status) => handleStatusChange(parent.id, status)}
								onStartHere={
									onStartIssue ? () => handleStartIssue(parent) : undefined
								}
								onStartNewSession={
									onStartIssueNewSession
										? () => handleStartIssueNewSession(parent)
										: undefined
								}
								onAddChild={() => handleAddChild(parent.id)}
								onEdit={() => handleStartEdit(parent)}
								isEditing={editingIssueId === parent.id}
								editTitle={editTitle}
								onEditTitleChange={setEditTitle}
								onEditSave={handleSaveEdit}
								onEditCancel={handleCancelEdit}
							/>
						))}
						{/* Then standalone issues */}
						{standaloneIssues.map((issue) => (
							<IssueCard
								key={issue.id}
								issue={issue}
								childrenByParent={childrenByParent}
								expandedIssues={expandedEpics}
								onToggleExpand={handleToggleEpic}
								isExpanded={false}
								onToggle={() => {}}
								onStatusChange={(status) =>
									handleStatusChange(issue.id, status)
								}
								onStartHere={
									onStartIssue ? () => handleStartIssue(issue) : undefined
								}
								onStartNewSession={
									onStartIssueNewSession
										? () => handleStartIssueNewSession(issue)
										: undefined
								}
								onAddChild={() => handleAddChild(issue.id)}
								onEdit={() => handleStartEdit(issue)}
								isEditing={editingIssueId === issue.id}
								editTitle={editTitle}
								onEditTitleChange={setEditTitle}
								onEditSave={handleSaveEdit}
								onEditCancel={handleCancelEdit}
							/>
						))}
					</>
				)}
			</div>
		</div>
	);
});
