"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { controlPlaneApiUrl } from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import {
	AlertCircle,
	Bug,
	ChevronDown,
	ChevronRight,
	CircleDot,
	ClipboardList,
	Loader2,
	Pencil,
	Plus,
	RefreshCw,
	Sparkles,
	Target,
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
		headers: { "Content-Type": "application/json" },
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
		headers: { "Content-Type": "application/json" },
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
		headers: { "Content-Type": "application/json" },
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
	feature: { icon: Sparkles, color: "text-purple-400", label: "Feature" },
	task: { icon: ClipboardList, color: "text-blue-400", label: "Task" },
	epic: { icon: Target, color: "text-amber-400", label: "Epic" },
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
	isExpanded,
	onToggle,
	onStatusChange,
	onEdit,
	depth = 0,
}: {
	issue: TrxIssue;
	childIssues?: TrxIssue[];
	isExpanded: boolean;
	onToggle: () => void;
	onStatusChange: (status: string) => void;
	onEdit: () => void;
	depth?: number;
}) {
	const typeConfig = issueTypeConfig[issue.issue_type] || issueTypeConfig.task;
	const TypeIcon = typeConfig.icon;
	const hasChildren = childIssues && childIssues.length > 0;
	const isClosed = issue.status === "closed";

	return (
		<div
			className={cn(
				"space-y-1",
				depth > 0 && "ml-4 border-l border-border pl-2",
			)}
		>
			<div
				className={cn(
					"group flex items-start gap-2 p-2 rounded transition-colors",
					isClosed ? "opacity-50" : "hover:bg-muted/50",
				)}
			>
				{/* Expand/collapse button for epics with children */}
				{hasChildren ? (
					<button
						type="button"
						onClick={onToggle}
						className="flex-shrink-0 mt-0.5 p-0.5 hover:bg-muted rounded"
					>
						{isExpanded ? (
							<ChevronDown className="w-3 h-3 text-muted-foreground" />
						) : (
							<ChevronRight className="w-3 h-3 text-muted-foreground" />
						)}
					</button>
				) : (
					<div className="w-4" />
				)}

				{/* Type icon */}
				<TypeIcon
					className={cn("w-4 h-4 flex-shrink-0 mt-0.5", typeConfig.color)}
				/>

				{/* Content */}
				<div className="flex-1 min-w-0">
					<div className="flex items-center gap-2">
						<span
							className={cn(
								"text-sm font-medium truncate",
								isClosed && "line-through text-muted-foreground",
							)}
						>
							{issue.title}
						</span>
						<span className="text-[10px] font-mono text-muted-foreground">
							{issue.id}
						</span>
					</div>
					{issue.description && (
						<p className="text-xs text-muted-foreground line-clamp-1 mt-0.5">
							{issue.description}
						</p>
					)}
				</div>

				{/* Status/priority badges */}
				<div className="flex items-center gap-1 flex-shrink-0">
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
				</div>

				{/* Actions - visible on hover */}
				<div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
					{!isClosed && (
						<>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={onEdit}
								className="h-5 w-5 p-0"
								title="Edit"
							>
								<Pencil className="w-3 h-3" />
							</Button>
							{issue.status !== "in_progress" && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={() => onStatusChange("in_progress")}
									className="h-5 w-5 p-0"
									title="Start"
								>
									<CircleDot className="w-3 h-3 text-purple-400" />
								</Button>
							)}
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => onStatusChange("closed")}
								className="h-5 w-5 p-0"
								title="Close"
							>
								<X className="w-3 h-3 text-green-400" />
							</Button>
						</>
					)}
				</div>
			</div>

			{/* Children (for epics) */}
			{hasChildren && isExpanded && (
				<div className="space-y-1">
					{childIssues.map((child) => (
						<IssueCard
							key={child.id}
							issue={child}
							isExpanded={false}
							onToggle={() => {}}
							onStatusChange={(status) => onStatusChange(status)}
							onEdit={onEdit}
							depth={depth + 1}
						/>
					))}
				</div>
			)}
		</div>
	);
});

export const TrxView = memo(function TrxView({
	workspacePath,
	className,
}: TrxViewProps) {
	const [issues, setIssues] = useState<TrxIssue[]>([]);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string>("");
	const [expandedEpics, setExpandedEpics] = useState<Set<string>>(new Set());
	const [showAddForm, setShowAddForm] = useState(false);
	const [newIssueTitle, setNewIssueTitle] = useState("");
	const [newIssueType, setNewIssueType] = useState("task");
	const [isCreating, setIsCreating] = useState(false);

	const loadIssues = useCallback(async () => {
		if (!workspacePath) return;

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

	useEffect(() => {
		loadIssues();
	}, [loadIssues]);

	// Organize issues into hierarchy (epics with children)
	const { epics, standaloneIssues, childrenByParent } = useMemo(() => {
		const childrenByParent = new Map<string, TrxIssue[]>();
		const standaloneIssues: TrxIssue[] = [];
		const epics: TrxIssue[] = [];

		// First pass: identify epics and build parent-child map
		for (const issue of issues) {
			if (issue.parent_id) {
				const existing = childrenByParent.get(issue.parent_id) || [];
				existing.push(issue);
				childrenByParent.set(issue.parent_id, existing);
			} else if (issue.issue_type === "epic") {
				epics.push(issue);
			} else {
				standaloneIssues.push(issue);
			}
		}

		return { epics, standaloneIssues, childrenByParent };
	}, [issues]);

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

	const handleCreate = useCallback(async () => {
		if (!workspacePath || !newIssueTitle.trim()) return;

		setIsCreating(true);
		setError("");
		try {
			await createTrxIssue(workspacePath, {
				title: newIssueTitle,
				issue_type: newIssueType,
			});
			setNewIssueTitle("");
			setShowAddForm(false);
			await loadIssues();
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to create issue");
		} finally {
			setIsCreating(false);
		}
	}, [workspacePath, newIssueTitle, newIssueType, loadIssues]);

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

	return (
		<div className={cn("flex flex-col h-full overflow-hidden", className)}>
			{/* Header */}
			<div className="flex-shrink-0 p-2 border-b border-border">
				<div className="flex items-center justify-between mb-2">
					<span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
						Issues
					</span>
					<div className="flex items-center gap-1">
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
							onClick={() => setShowAddForm(!showAddForm)}
							className="h-6 w-6 p-0"
							title="Add issue"
						>
							<Plus className="w-3 h-3" />
						</Button>
					</div>
				</div>

				{/* Stats bar */}
				{stats.total > 0 && (
					<div className="flex items-center gap-3 text-[10px] text-muted-foreground">
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
						<Input
							value={newIssueTitle}
							onChange={(e) => setNewIssueTitle(e.target.value)}
							placeholder="Issue title..."
							className="h-7 text-xs"
							onKeyDown={(e) => e.key === "Enter" && handleCreate()}
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
								onClick={() => setShowAddForm(false)}
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
			<div className="flex-1 overflow-auto p-2 space-y-1">
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
						{/* Epics first */}
						{epics.map((epic) => (
							<IssueCard
								key={epic.id}
								issue={epic}
								childIssues={childrenByParent.get(epic.id)}
								isExpanded={expandedEpics.has(epic.id)}
								onToggle={() => handleToggleEpic(epic.id)}
								onStatusChange={(status) => handleStatusChange(epic.id, status)}
								onEdit={() => {}}
							/>
						))}
						{/* Then standalone issues */}
						{standaloneIssues.map((issue) => (
							<IssueCard
								key={issue.id}
								issue={issue}
								isExpanded={false}
								onToggle={() => {}}
								onStatusChange={(status) =>
									handleStatusChange(issue.id, status)
								}
								onEdit={() => {}}
							/>
						))}
					</>
				)}
			</div>
		</div>
	);
});
