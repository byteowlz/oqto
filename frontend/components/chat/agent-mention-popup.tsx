"use client";

import { cn } from "@/lib/utils";
import { Bot, FolderPlus, Loader2, MessageSquare } from "lucide-react";
import { memo, useCallback, useEffect, useRef, useState } from "react";

/** Check if a string looks like a directory path */
function looksLikePath(query: string): boolean {
	if (!query) return false;
	// Starts with / or ~ or contains /
	return query.startsWith("/") || query.startsWith("~") || query.includes("/");
}

/** Expand ~ to home directory placeholder (actual expansion happens server-side) */
function expandPath(path: string): string {
	// Keep ~ as-is - server will expand it
	return path;
}

export interface AgentTarget {
	id: string;
	name: string;
	type: "default-chat" | "session" | "new-session";
	title?: string;
	description?: string;
	/** Workspace path for filtering or for new session creation */
	workspace_path?: string;
	/** Project name for filtering */
	project_name?: string;
}

interface SessionInfo {
	id: string;
	title?: string | null;
	workspace_path?: string;
	project_name?: string;
}

interface AgentMentionPopupProps {
	query: string;
	isOpen: boolean;
	defaultChatName?: string;
	/** Default Chat workspace path (for filtering Default Chat by path) */
	defaultChatWorkspacePath?: string;
	sessions?: SessionInfo[];
	onSelect: (target: AgentTarget) => void;
	onClose: () => void;
	className?: string;
}

// Simple fuzzy match
function fuzzyMatch(query: string, text: string): boolean {
	if (!query) return true;
	const lowerQuery = query.toLowerCase();
	const lowerText = text.toLowerCase();

	let qi = 0;
	for (let i = 0; i < lowerText.length && qi < lowerQuery.length; i++) {
		if (lowerText[i] === lowerQuery[qi]) {
			qi++;
		}
	}
	return qi === lowerQuery.length;
}

// Score a match (higher = better)
function matchScore(query: string, text: string): number {
	if (!query) return 0;
	const lowerQuery = query.toLowerCase();
	const lowerText = text.toLowerCase();

	if (lowerText.startsWith(lowerQuery)) return 100;
	if (lowerText.includes(lowerQuery)) return 50;
	return 10;
}

export const AgentMentionPopup = memo(function AgentMentionPopup({
	query,
	isOpen,
	defaultChatName,
	defaultChatWorkspacePath,
	sessions = [],
	onSelect,
	onClose,
	className,
}: AgentMentionPopupProps) {
	const [selectedIndex, setSelectedIndex] = useState(0);
	const listRef = useRef<HTMLDivElement>(null);

	// Build list of targets from sessions
	// Each session becomes a target, showing session title and project info
	const sessionTargets: AgentTarget[] = sessions.map((session) => ({
		id: session.id,
		name:
			session.title ||
			session.project_name ||
			session.workspace_path?.split("/").pop() ||
			session.id.slice(0, 12),
		type: "session" as const,
		title: session.title || undefined,
		description: session.workspace_path,
		workspace_path: session.workspace_path,
		project_name: session.project_name,
	}));

	// Filter and sort based on query
	// When query matches a project/directory name, show sessions from that project
	// Otherwise show all sessions that match the query
	const filteredTargets = (() => {
		const targets: AgentTarget[] = [];

		// Add default chat as first option (if it matches query or no query)
		if (defaultChatName) {
			const defaultChatTarget: AgentTarget = {
				id: "default-chat",
				name: defaultChatName,
				type: "default-chat",
				title: "Default Chat",
				description:
					defaultChatWorkspacePath?.split("/").pop() ||
					"Ask the default chat assistant",
				workspace_path: defaultChatWorkspacePath,
			};

			const defaultChatFields = [
				defaultChatName,
				"main",
				"default-chat",
				defaultChatWorkspacePath?.split("/").pop() || "",
			];

			if (!query || defaultChatFields.some((f) => fuzzyMatch(query, f))) {
				targets.push(defaultChatTarget);
			}
		}

		if (!query) {
			// No query - show default chat + one entry per unique project (most recent session)
			const seenProjects = new Set<string>();
			for (const t of sessionTargets) {
				const key = t.workspace_path || t.id;
				if (!seenProjects.has(key)) {
					seenProjects.add(key);
					targets.push(t);
				}
			}
			return targets.slice(0, 15);
		}

		// Check if query matches a project/directory name
		const queryLower = query.toLowerCase();
		const matchingProjects = new Set<string>();

		for (const t of sessionTargets) {
			const dirName = t.workspace_path?.split("/").pop()?.toLowerCase() || "";
			const projectName = t.project_name?.toLowerCase() || "";

			if (dirName.includes(queryLower) || projectName.includes(queryLower)) {
				if (t.workspace_path) {
					matchingProjects.add(t.workspace_path);
				}
			}
		}

		// If query matches project names, show all sessions from those projects
		if (matchingProjects.size > 0) {
			for (const t of sessionTargets) {
				if (t.workspace_path && matchingProjects.has(t.workspace_path)) {
					targets.push(t);
				}
			}
		} else {
			// Query doesn't match any project - search across all fields
			for (const t of sessionTargets) {
				const searchFields = [
					t.name,
					t.title || "",
					t.id,
					t.project_name || "",
					t.workspace_path || "",
					t.workspace_path?.split("/").pop() || "",
				];
				if (searchFields.some((field) => fuzzyMatch(query, field))) {
					targets.push(t);
				}
			}
		}

		// Sort by match quality
		const sorted = targets
			.sort((a, b) => {
				const scoreFields = (t: AgentTarget) =>
					Math.max(
						matchScore(query, t.name),
						matchScore(query, t.title || ""),
						matchScore(query, t.project_name || ""),
						matchScore(query, t.workspace_path?.split("/").pop() || ""),
					);
				return scoreFields(b) - scoreFields(a);
			})
			.slice(0, 14); // Leave room for new session option

		// Add "New session" option if query looks like a path
		if (looksLikePath(query)) {
			const expandedPath = expandPath(query);
			const dirName = expandedPath.split("/").pop() || expandedPath;

			// Check if we already have a session for this exact path
			const hasExactMatch = sessionTargets.some(
				(t) => t.workspace_path === expandedPath,
			);

			// Always show "New session" option for path-like queries
			// It will create a new session even if one exists for the path
			sorted.unshift({
				id: `new-session:${expandedPath}`,
				name: dirName,
				type: "new-session",
				title: hasExactMatch ? "New session" : "Start session",
				description: expandedPath,
				workspace_path: expandedPath,
			});
		}

		return sorted.slice(0, 15);
	})();

	// Reset selection when query changes
	const prevQueryRef = useRef(query);
	if (prevQueryRef.current !== query) {
		prevQueryRef.current = query;
		if (selectedIndex !== 0) {
			setSelectedIndex(0);
		}
	}

	// Scroll selected item into view
	useEffect(() => {
		if (!listRef.current) return;
		const selectedEl = listRef.current.querySelector(
			`[data-index="${selectedIndex}"]`,
		);
		if (selectedEl) {
			selectedEl.scrollIntoView({ block: "nearest" });
		}
	}, [selectedIndex]);

	// Handle keyboard navigation
	useEffect(() => {
		if (!isOpen) return;

		const handleKeyDown = (e: KeyboardEvent) => {
			switch (e.key) {
				case "ArrowDown":
					e.preventDefault();
					e.stopPropagation();
					setSelectedIndex((prev) =>
						prev < filteredTargets.length - 1 ? prev + 1 : prev,
					);
					break;
				case "ArrowUp":
					e.preventDefault();
					e.stopPropagation();
					setSelectedIndex((prev) => (prev > 0 ? prev - 1 : prev));
					break;
				case "Enter":
				case "Tab":
					if (filteredTargets[selectedIndex]) {
						e.preventDefault();
						e.stopPropagation();
						onSelect(filteredTargets[selectedIndex]);
					}
					break;
				case "Escape":
					e.preventDefault();
					e.stopPropagation();
					onClose();
					break;
			}
		};

		document.addEventListener("keydown", handleKeyDown, true);
		return () => document.removeEventListener("keydown", handleKeyDown, true);
	}, [isOpen, filteredTargets, selectedIndex, onSelect, onClose]);

	if (!isOpen) return null;

	return (
		<div
			ref={listRef}
			className={cn(
				"absolute bottom-full left-0 mb-2 w-80 max-h-64 overflow-y-auto",
				"bg-popover border border-border rounded-lg shadow-lg",
				"z-50",
				className,
			)}
		>
			<div className="p-1">
				{/* Header */}
				<div className="px-3 py-1.5 text-xs text-muted-foreground border-b border-border mb-1">
					{query ? `Agents matching "${query}"` : "Ask another agent"}
				</div>

				{filteredTargets.length === 0 && (
					<div className="px-3 py-4 text-sm text-muted-foreground text-center">
						No agents found
					</div>
				)}

				{filteredTargets.map((target, index) => (
					<button
						type="button"
						key={target.id}
						data-index={index}
						onClick={() => onSelect(target)}
						onMouseEnter={() => setSelectedIndex(index)}
						className={cn(
							"w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-left",
							"transition-colors",
							index === selectedIndex
								? "bg-accent text-accent-foreground"
								: "hover:bg-muted",
						)}
					>
						{target.type === "default-chat" ? (
							<Bot className="w-4 h-4 text-primary shrink-0" />
						) : target.type === "new-session" ? (
							<FolderPlus className="w-4 h-4 text-green-500 shrink-0" />
						) : (
							<MessageSquare className="w-4 h-4 text-muted-foreground shrink-0" />
						)}
						<div className="flex-1 min-w-0">
							<div className="text-sm font-medium truncate">{target.name}</div>
							{(target.type === "session" || target.type === "new-session") && (
								<div className="text-xs text-muted-foreground truncate">
									{target.type === "new-session"
										? target.description
										: target.project_name ||
											target.workspace_path?.split("/").pop() ||
											"Session"}
								</div>
							)}
						</div>
						<span className="text-xs text-muted-foreground shrink-0">
							{target.type === "default-chat"
								? "Main"
								: target.type === "new-session"
									? target.title
									: "Session"}
						</span>
					</button>
				))}
			</div>
		</div>
	);
});

// Chip for displaying selected agent target
export const AgentTargetChip = memo(function AgentTargetChip({
	target,
	onRemove,
}: {
	target: AgentTarget;
	onRemove: () => void;
}) {
	const isMac =
		typeof navigator !== "undefined" &&
		navigator.platform.toLowerCase().includes("mac");
	const modKey = isMac ? "Cmd" : "Ctrl";

	const isNewSession = target.type === "new-session";
	const colorClass = isNewSession
		? "bg-green-500/10 text-green-500"
		: "bg-blue-500/10 text-blue-500";

	return (
		<span
			className={cn(
				"inline-flex items-center gap-1 px-2 py-0.5 rounded text-sm",
				colorClass,
			)}
			title={
				isNewSession
					? `New session in ${target.workspace_path}`
					: `Enter: ask and inject reply | ${modKey}+Enter: ask without reply`
			}
		>
			{target.type === "default-chat" ? (
				<Bot className="w-3 h-3" />
			) : target.type === "new-session" ? (
				<FolderPlus className="w-3 h-3" />
			) : (
				<MessageSquare className="w-3 h-3" />
			)}
			<span className="max-w-[120px] truncate">@@{target.name}</span>
			<button
				type="button"
				onClick={onRemove}
				className="ml-0.5 hover:text-destructive"
				title="Remove"
			>
				x
			</button>
		</span>
	);
});
