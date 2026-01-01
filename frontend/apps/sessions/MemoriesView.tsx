"use client";

import { useApp } from "@/components/app-context";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import {
	Brain,
	Calendar,
	Loader2,
	Pencil,
	Plus,
	RefreshCw,
	Save,
	Search,
	Sparkles,
	Tag,
	Trash2,
	X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

// Memory type from mmry API
interface Memory {
	id: string;
	memory_type: string;
	content: string;
	metadata: Record<string, unknown>;
	importance: number;
	expires_at?: string;
	expired_at?: string;
	created_at: string;
	updated_at: string;
	category: string;
	tags: string[];
	parent_id?: string;
	chunk_index?: number;
	total_chunks?: number;
	chunk_method?: string;
}

interface MemoryListResponse {
	memories: Memory[];
	total: number;
	offset: number;
	limit: number;
}

interface SearchResponse {
	memories: Memory[];
	guardrails?: {
		blocked_memories: number;
		blocked_facts: number;
		triggered_patterns: string[];
	};
}

interface MemoriesViewProps {
	className?: string;
}

// API functions
async function fetchMemories(
	sessionId: string,
	offset = 0,
	limit = 50,
): Promise<MemoryListResponse> {
	const url = new URL(`/api/session/${sessionId}/memories`, window.location.origin);
	url.searchParams.set("limit", limit.toString());
	url.searchParams.set("offset", offset.toString());

	const res = await fetch(url.toString(), {
		credentials: "include",
	});
	if (!res.ok) {
		if (res.status === 404) {
			// mmry not enabled or not available
			return { memories: [], total: 0, offset: 0, limit: 50 };
		}
		throw new Error(`Failed to fetch memories: ${res.statusText}`);
	}
	return res.json();
}

async function searchMemories(
	sessionId: string,
	query: string,
	limit = 50,
): Promise<Memory[]> {
	const res = await fetch(`/api/session/${sessionId}/memories/search`, {
		method: "POST",
		credentials: "include",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			query,
			limit,
			rerank: true,
		}),
	});
	if (!res.ok) {
		throw new Error(`Failed to search memories: ${res.statusText}`);
	}
	const data: SearchResponse = await res.json();
	return data.memories || [];
}

async function addMemory(
	sessionId: string,
	content: string,
	category?: string,
	tags?: string[],
	importance?: number,
): Promise<Memory> {
	const res = await fetch(`/api/session/${sessionId}/memories`, {
		method: "POST",
		credentials: "include",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			content,
			category: category || "general",
			tags: tags || [],
			importance: importance || 5,
		}),
	});
	if (!res.ok) {
		const text = await res.text();
		throw new Error(`Failed to add memory: ${text || res.statusText}`);
	}
	return res.json();
}

async function deleteMemory(
	sessionId: string,
	memoryId: string,
): Promise<void> {
	const res = await fetch(
		`/api/session/${sessionId}/memories/${memoryId}`,
		{
			method: "DELETE",
			credentials: "include",
		},
	);
	if (!res.ok) {
		throw new Error(`Failed to delete memory: ${res.statusText}`);
	}
}

async function updateMemory(
	sessionId: string,
	memoryId: string,
	content: string,
	category?: string,
	tags?: string[],
	importance?: number,
): Promise<Memory> {
	const res = await fetch(
		`/api/session/${sessionId}/memories/${memoryId}`,
		{
			method: "PUT",
			credentials: "include",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				content,
				...(category && { category }),
				...(tags && { tags }),
				...(importance && { importance }),
			}),
		},
	);
	if (!res.ok) {
		throw new Error(`Failed to update memory: ${res.statusText}`);
	}
	return res.json();
}

function MemoryCard({
	memory,
	onDelete,
	onEdit,
	isDeleting,
}: {
	memory: Memory;
	onDelete: () => void;
	onEdit: (content: string) => void;
	isDeleting: boolean;
}) {
	const [isEditing, setIsEditing] = useState(false);
	const [editContent, setEditContent] = useState(memory.content);
	const [isSaving, setIsSaving] = useState(false);
	const textareaRef = useRef<HTMLTextAreaElement>(null);

	// Auto-resize textarea to fit content
	useEffect(() => {
		if (isEditing && textareaRef.current) {
			const textarea = textareaRef.current;
			if (editContent.length === 0) {
				textarea.style.height = "auto";
			}
			textarea.style.height = "auto";
			textarea.style.height = `${textarea.scrollHeight}px`;
			textarea.focus();
		}
	}, [isEditing, editContent]);

	const handleSave = async () => {
		setIsSaving(true);
		try {
			await onEdit(editContent);
			setIsEditing(false);
		} finally {
			setIsSaving(false);
		}
	};

	const handleCancel = () => {
		setEditContent(memory.content);
		setIsEditing(false);
	};

	const createdAt = new Date(memory.created_at);
	const formattedDate = createdAt.toLocaleDateString(undefined, {
		month: "short",
		day: "numeric",
		year:
			createdAt.getFullYear() !== new Date().getFullYear()
				? "numeric"
				: undefined,
	});

	return (
		<div className="group border border-border rounded-lg p-3 hover:bg-muted/30 transition-colors">
			{isEditing ? (
				<div className="space-y-2">
					<textarea
						ref={textareaRef}
						value={editContent}
						onChange={(e) => setEditContent(e.target.value)}
						className="w-full min-h-[60px] p-2 text-sm bg-background border border-border rounded resize-none focus:outline-none focus:ring-1 focus:ring-primary overflow-hidden"
					/>
					<div className="flex justify-end gap-1">
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={handleCancel}
							disabled={isSaving}
							className="h-7 px-2 text-xs"
						>
							<X className="w-3 h-3 mr-1" />
							Cancel
						</Button>
						<Button
							type="button"
							variant="default"
							size="sm"
							onClick={handleSave}
							disabled={isSaving}
							className="h-7 px-2 text-xs"
						>
							{isSaving ? (
								<Loader2 className="w-3 h-3 mr-1 animate-spin" />
							) : (
								<Save className="w-3 h-3 mr-1" />
							)}
							Save
						</Button>
					</div>
				</div>
			) : (
				<>
					<p className="text-sm whitespace-pre-wrap break-words">
						{memory.content}
					</p>

					<div className="mt-2 flex items-center justify-between">
						<div className="flex items-center gap-2 text-xs text-muted-foreground flex-wrap">
							<span className="flex items-center gap-1">
								<Calendar className="w-3 h-3" />
								{formattedDate}
							</span>
							{memory.category && memory.category !== "general" && (
								<Badge variant="secondary" className="text-[10px] px-1.5 py-0">
									{memory.category}
								</Badge>
							)}
							{memory.importance > 5 && (
								<Badge variant="outline" className="text-[10px] px-1.5 py-0">
									importance: {memory.importance}
								</Badge>
							)}
							{memory.tags && memory.tags.length > 0 && (
								<span className="flex items-center gap-1">
									<Tag className="w-3 h-3" />
									{memory.tags.slice(0, 2).join(", ")}
									{memory.tags.length > 2 && `+${memory.tags.length - 2}`}
								</span>
							)}
						</div>

						<div className="flex items-center gap-0.5 lg:opacity-0 lg:group-hover:opacity-100 transition-opacity">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => setIsEditing(true)}
								className="h-6 w-6 p-0"
								title="Edit"
							>
								<Pencil className="w-3 h-3" />
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={onDelete}
								disabled={isDeleting}
								className="h-6 w-6 p-0 text-destructive hover:text-destructive"
								title="Delete"
							>
								{isDeleting ? (
									<Loader2 className="w-3 h-3 animate-spin" />
								) : (
									<Trash2 className="w-3 h-3" />
								)}
							</Button>
						</div>
					</div>
				</>
			)}
		</div>
	);
}

export function MemoriesView({ className }: MemoriesViewProps) {
	const { selectedWorkspaceSessionId } = useApp();
	const [memories, setMemories] = useState<Memory[]>([]);
	const [total, setTotal] = useState(0);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string>("");
	const [searchQuery, setSearchQuery] = useState("");
	const [isSearching, setIsSearching] = useState(false);
	const [isSearchMode, setIsSearchMode] = useState(false);
	const [showAddForm, setShowAddForm] = useState(false);
	const [newMemoryContent, setNewMemoryContent] = useState("");
	const [isAdding, setIsAdding] = useState(false);
	const [deletingId, setDeletingId] = useState<string | null>(null);
	const addTextareaRef = useRef<HTMLTextAreaElement>(null);

	const loadMemories = useCallback(async () => {
		if (!selectedWorkspaceSessionId) return;

		setLoading(true);
		setError("");
		setIsSearchMode(false);
		try {
			const data = await fetchMemories(selectedWorkspaceSessionId);
			setMemories(data.memories);
			setTotal(data.total);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to load memories");
		} finally {
			setLoading(false);
		}
	}, [selectedWorkspaceSessionId]);

	useEffect(() => {
		loadMemories();
	}, [loadMemories]);

	useEffect(() => {
		if (showAddForm && addTextareaRef.current) {
			addTextareaRef.current.focus();
		}
	}, [showAddForm]);

	const handleSearch = useCallback(async () => {
		if (!selectedWorkspaceSessionId) return;

		if (!searchQuery.trim()) {
			loadMemories();
			return;
		}

		setIsSearching(true);
		setError("");
		setIsSearchMode(true);
		try {
			const results = await searchMemories(
				selectedWorkspaceSessionId,
				searchQuery,
			);
			setMemories(results);
			setTotal(results.length);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Search failed");
		} finally {
			setIsSearching(false);
		}
	}, [selectedWorkspaceSessionId, searchQuery, loadMemories]);

	const handleClearSearch = useCallback(() => {
		setSearchQuery("");
		loadMemories();
	}, [loadMemories]);

	const handleAdd = useCallback(async () => {
		if (!selectedWorkspaceSessionId || !newMemoryContent.trim()) return;

		setIsAdding(true);
		setError("");
		try {
			const newMemory = await addMemory(
				selectedWorkspaceSessionId,
				newMemoryContent,
			);
			setMemories((prev) => [newMemory, ...prev]);
			setTotal((prev) => prev + 1);
			setNewMemoryContent("");
			setShowAddForm(false);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to add memory");
		} finally {
			setIsAdding(false);
		}
	}, [selectedWorkspaceSessionId, newMemoryContent]);

	const handleDelete = useCallback(
		async (memoryId: string) => {
			if (!selectedWorkspaceSessionId) return;

			setDeletingId(memoryId);
			try {
				await deleteMemory(selectedWorkspaceSessionId, memoryId);
				setMemories((prev) => prev.filter((m) => m.id !== memoryId));
				setTotal((prev) => prev - 1);
			} catch (err) {
				setError(
					err instanceof Error ? err.message : "Failed to delete memory",
				);
			} finally {
				setDeletingId(null);
			}
		},
		[selectedWorkspaceSessionId],
	);

	const handleEdit = useCallback(
		async (memoryId: string, content: string) => {
			if (!selectedWorkspaceSessionId) return;

			try {
				const updated = await updateMemory(
					selectedWorkspaceSessionId,
					memoryId,
					content,
				);
				setMemories((prev) =>
					prev.map((m) => (m.id === memoryId ? updated : m)),
				);
			} catch (err) {
				setError(
					err instanceof Error ? err.message : "Failed to update memory",
				);
				throw err; // Re-throw so the card knows to not exit edit mode
			}
		},
		[selectedWorkspaceSessionId],
	);

	// No session selected
	if (!selectedWorkspaceSessionId) {
		return (
			<div
				className={cn(
					"h-full bg-muted/30 rounded flex items-center justify-center",
					className,
				)}
			>
				<div className="text-center text-muted-foreground">
					<Brain className="w-12 h-12 mx-auto mb-2 opacity-50" />
					<p className="text-sm">No session selected</p>
				</div>
			</div>
		);
	}

	// Loading state
	if (loading) {
		return (
			<div
				className={cn(
					"h-full bg-muted/30 rounded flex items-center justify-center",
					className,
				)}
			>
				<div className="text-center text-muted-foreground">
					<Loader2 className="w-8 h-8 mx-auto mb-2 animate-spin" />
					<p className="text-sm">Loading memories...</p>
				</div>
			</div>
		);
	}

	return (
		<div className={cn("h-full flex flex-col overflow-hidden", className)}>
			{/* Header with search and add */}
			<div className="flex-shrink-0 p-2 border-b border-border space-y-2">
				<div className="flex items-center gap-2">
					<div className="relative flex-1">
						<Search className="absolute left-2 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
						<Input
							value={searchQuery}
							onChange={(e) => setSearchQuery(e.target.value)}
							onKeyDown={(e) => e.key === "Enter" && handleSearch()}
							placeholder="Search memories..."
							className="pl-8 h-8 text-sm"
						/>
					</div>
					{isSearchMode ? (
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={handleClearSearch}
							className="h-8"
							title="Clear search"
						>
							<X className="w-4 h-4" />
						</Button>
					) : (
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={loadMemories}
							disabled={loading}
							className="h-8"
							title="Refresh"
						>
							<RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
						</Button>
					)}
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={handleSearch}
						disabled={isSearching || !searchQuery.trim()}
						className="h-8"
					>
						{isSearching ? (
							<Loader2 className="w-4 h-4 animate-spin" />
						) : (
							<Search className="w-4 h-4" />
						)}
					</Button>
					<Button
						type="button"
						variant="default"
						size="sm"
						onClick={() => setShowAddForm(!showAddForm)}
						className="h-8"
					>
						<Plus className="w-4 h-4" />
					</Button>
				</div>

				{/* Add memory form */}
				{showAddForm && (
					<div className="space-y-2 p-2 bg-muted/30 rounded-lg">
						<textarea
							ref={addTextareaRef}
							value={newMemoryContent}
							onChange={(e) => setNewMemoryContent(e.target.value)}
							placeholder="Enter a new memory..."
							className="w-full min-h-[60px] p-2 text-sm bg-background border border-border rounded resize-none focus:outline-none focus:ring-1 focus:ring-primary"
						/>
						<div className="flex justify-end gap-1">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => {
									setShowAddForm(false);
									setNewMemoryContent("");
								}}
								className="h-7 px-2 text-xs"
							>
								Cancel
							</Button>
							<Button
								type="button"
								variant="default"
								size="sm"
								onClick={handleAdd}
								disabled={isAdding || !newMemoryContent.trim()}
								className="h-7 px-2 text-xs"
							>
								{isAdding ? (
									<Loader2 className="w-3 h-3 mr-1 animate-spin" />
								) : (
									<Sparkles className="w-3 h-3 mr-1" />
								)}
								Add Memory
							</Button>
						</div>
					</div>
				)}

				{/* Status bar */}
				{!loading && (
					<div className="text-xs text-muted-foreground">
						{isSearchMode ? (
							<span>
								Found {memories.length} result{memories.length !== 1 ? "s" : ""}{" "}
								for "{searchQuery}"
							</span>
						) : (
							<span>
								{total} memor{total !== 1 ? "ies" : "y"}
							</span>
						)}
					</div>
				)}
			</div>

			{/* Error message */}
			{error && (
				<div className="flex-shrink-0 px-3 py-2 bg-destructive/10 text-destructive text-xs">
					{error}
				</div>
			)}

			{/* Memories list */}
			<div className="flex-1 overflow-auto p-2 space-y-2">
				{memories.length === 0 ? (
					<div className="h-full flex items-center justify-center">
						<div className="text-center text-muted-foreground">
							<Brain className="w-12 h-12 mx-auto mb-2 opacity-50" />
							{isSearchMode ? (
								<>
									<p className="text-sm">No memories found</p>
									<p className="text-xs mt-1">Try a different search query</p>
								</>
							) : (
								<>
									<p className="text-sm">No memories yet</p>
									<p className="text-xs mt-1">Add your first memory above</p>
								</>
							)}
						</div>
					</div>
				) : (
					memories.map((memory) => (
						<MemoryCard
							key={memory.id}
							memory={memory}
							onDelete={() => handleDelete(memory.id)}
							onEdit={(content) => handleEdit(memory.id, content)}
							isDeleting={deletingId === memory.id}
						/>
					))
				)}
			</div>
		</div>
	);
}
