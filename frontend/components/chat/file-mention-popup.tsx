"use client";

import { fetchFileTreeMux } from "@/lib/mux-files";
import { normalizeWorkspacePath } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import { File, Folder, Loader2 } from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";

export interface FileAttachment {
	id: string;
	path: string;
	filename: string;
	type: "file";
}

export interface IssueAttachment {
	id: string;
	issueId: string;
	title: string;
	description?: string;
	type: "issue";
}

interface FileNode {
	name: string;
	path: string;
	type: "file" | "directory";
	children?: FileNode[];
}

interface FileMentionPopupProps {
	query: string;
	isOpen: boolean;
	workspacePath: string | null;
	onSelect: (file: FileAttachment) => void;
	onClose: () => void;
	className?: string;
}

// Simple fuzzy match
function fuzzyMatch(query: string, text: string): boolean {
	if (!query) return true;
	const lowerQuery = query.toLowerCase();
	const lowerText = text.toLowerCase();

	// Check if all characters appear in order
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

	// Exact match at start
	if (lowerText.startsWith(lowerQuery)) return 100;
	// Contains
	if (lowerText.includes(lowerQuery)) return 50;
	// Fuzzy
	return 10;
}

// Recursively collect all files from a tree
function collectAllFiles(nodes: FileNode[], prefix = ""): FileNode[] {
	const result: FileNode[] = [];

	for (const node of nodes) {
		const fullPath = prefix ? `${prefix}/${node.name}` : node.name;
		if (node.type === "file") {
			result.push({ ...node, path: fullPath });
		}
		if (node.children && node.children.length > 0) {
			result.push(...collectAllFiles(node.children, fullPath));
		}
	}

	return result;
}

export const FileMentionPopup = memo(function FileMentionPopup({
	query,
	isOpen,
	workspacePath,
	onSelect,
	onClose,
	className,
}: FileMentionPopupProps) {
	const [files, setFiles] = useState<FileNode[]>([]);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [selectedIndex, setSelectedIndex] = useState(0);
	const listRef = useRef<HTMLDivElement>(null);
	const normalizedWorkspacePath = useMemo(
		() => normalizeWorkspacePath(workspacePath),
		[workspacePath],
	);

	// Load files when popup opens
	useEffect(() => {
		if (!isOpen || !normalizedWorkspacePath) {
			setFiles([]);
			return;
		}

		setLoading(true);
		setError(null);

		fetchFileTreeMux(normalizedWorkspacePath, ".", 10, false)
			.then((data) => {
				// Flatten and collect all files
				const allFiles = collectAllFiles(data);
				setFiles(allFiles);
			})
			.catch((err) => {
				setError(err.message);
			})
			.finally(() => {
				setLoading(false);
			});
	}, [isOpen, workspacePath]);

	// Filter and sort files based on query
	const filteredFiles = files
		.filter((f) => fuzzyMatch(query, f.path))
		.sort((a, b) => matchScore(query, b.path) - matchScore(query, a.path))
		.slice(0, 20); // Limit to 20 results

	// Reset selection when query changes - use ref to avoid dependency issues
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

	// Handle file selection
	const handleSelect = useCallback(
		(file: FileNode) => {
			const attachment: FileAttachment = {
				id: `file-${Date.now()}-${Math.random().toString(36).slice(2)}`,
				path: file.path,
				filename: file.name,
				type: "file",
			};
			onSelect(attachment);
		},
		[onSelect],
	);

	// Handle keyboard navigation
	useEffect(() => {
		if (!isOpen) return;

		const handleKeyDown = (e: KeyboardEvent) => {
			switch (e.key) {
				case "ArrowDown":
					e.preventDefault();
					e.stopPropagation();
					setSelectedIndex((prev) =>
						prev < filteredFiles.length - 1 ? prev + 1 : prev,
					);
					break;
				case "ArrowUp":
					e.preventDefault();
					e.stopPropagation();
					setSelectedIndex((prev) => (prev > 0 ? prev - 1 : prev));
					break;
				case "Enter":
				case "Tab":
					if (filteredFiles[selectedIndex]) {
						e.preventDefault();
						e.stopPropagation();
						handleSelect(filteredFiles[selectedIndex]);
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
	}, [isOpen, filteredFiles, selectedIndex, handleSelect, onClose]);

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
					{query ? `Files matching "${query}"` : "Recent files"}
				</div>

				{loading && (
					<div className="flex items-center justify-center py-4">
						<Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
						<span className="ml-2 text-sm text-muted-foreground">
							Loading files...
						</span>
					</div>
				)}

				{error && (
					<div className="px-3 py-2 text-sm text-destructive">{error}</div>
				)}

				{!loading && !error && filteredFiles.length === 0 && (
					<div className="px-3 py-4 text-sm text-muted-foreground text-center">
						No files found
					</div>
				)}

				{!loading &&
					!error &&
					filteredFiles.map((file, index) => (
						<button
							type="button"
							key={file.path}
							data-index={index}
							onClick={() => handleSelect(file)}
							onMouseEnter={() => setSelectedIndex(index)}
							className={cn(
								"w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-left",
								"transition-colors",
								index === selectedIndex
									? "bg-accent text-accent-foreground"
									: "hover:bg-muted",
							)}
						>
							<File className="w-4 h-4 text-muted-foreground shrink-0" />
							<div className="flex-1 min-w-0">
								<div className="text-sm truncate">{file.path}</div>
							</div>
						</button>
					))}
			</div>
		</div>
	);
});

// Attachment chip component for displaying selected files
export const FileAttachmentChip = memo(function FileAttachmentChip({
	attachment,
	onRemove,
}: {
	attachment: FileAttachment;
	onRemove: () => void;
}) {
	return (
		<span className="inline-flex items-center gap-1 px-2 py-0.5 bg-primary/10 text-primary rounded text-sm">
			<File className="w-3 h-3" />
			<span className="max-w-[150px] truncate">@{attachment.filename}</span>
			<button
				type="button"
				onClick={onRemove}
				className="ml-0.5 hover:text-destructive"
				title="Remove"
			>
				×
			</button>
		</span>
	);
});

export const IssueAttachmentChip = memo(function IssueAttachmentChip({
	attachment,
	onRemove,
}: {
	attachment: IssueAttachment;
	onRemove: () => void;
}) {
	return (
		<span className="inline-flex items-center gap-1 px-2 py-0.5 bg-purple-500/10 text-purple-500 rounded text-sm">
			<span className="font-mono text-xs">#{attachment.issueId}</span>
			<span className="max-w-[120px] truncate">{attachment.title}</span>
			<button
				type="button"
				onClick={onRemove}
				className="ml-0.5 hover:text-destructive"
				title="Remove"
			>
				×
			</button>
		</span>
	);
});
