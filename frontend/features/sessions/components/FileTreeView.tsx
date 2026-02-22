"use client";

import { FileIcon } from "@/components/data-display";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	copyPathMux,
	createDirectoryMux,
	deletePathMux,
	downloadFileMux,
	fetchFileTreeMux,
	movePathMux,
	renamePathMux,
	uploadFileMux,
} from "@/lib/mux-files";
import { normalizeWorkspacePath } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	ChevronDown,
	ChevronRight,
	Copy,
	Download,
	Folder,
	FolderPlus,
	FolderUp,
	Home,
	LayoutGrid,
	List,
	Loader2,
	MoveRight,
	PaintBucket,
	Pencil,
	Trash2,
	Upload,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";

export type FileNode = {
	name: string;
	path: string;
	type: "file" | "directory";
	size?: number;
	modified?: number;
	children?: FileNode[];
};

// Cache for file tree data - keyed by workspace:path:depth
const treeCache = new Map<string, { data: FileNode[]; timestamp: number }>();
const TREE_CACHE_TTL_MS = 30000; // 30 seconds
const treeInFlight = new Map<string, Promise<FileNode[]>>();

// Initial load depth (shallow for speed) and lazy-load depth on expand
const INITIAL_DEPTH = 2;
const LAZY_LOAD_DEPTH = 2;

function getTreeCacheKey(
	workspaceKey: string,
	path: string,
	depth = INITIAL_DEPTH,
): string {
	return `${workspaceKey}:${path}:${depth}`;
}

function getCachedTree(
	workspaceKey: string,
	path: string,
	depth = INITIAL_DEPTH,
): FileNode[] | null {
	const key = getTreeCacheKey(workspaceKey, path, depth);
	const entry = treeCache.get(key);
	if (!entry) return null;
	if (Date.now() - entry.timestamp > TREE_CACHE_TTL_MS) {
		treeCache.delete(key);
		return null;
	}
	return entry.data;
}

function setCachedTree(
	workspaceKey: string,
	path: string,
	data: FileNode[],
	depth = INITIAL_DEPTH,
): void {
	const key = getTreeCacheKey(workspaceKey, path, depth);
	// Limit cache size
	if (treeCache.size >= 50) {
		const firstKey = treeCache.keys().next().value;
		if (firstKey) treeCache.delete(firstKey);
	}
	treeCache.set(key, { data, timestamp: Date.now() });
}

async function fetchFileTree(
	workspacePath: string,
	path = ".",
	depth = INITIAL_DEPTH,
): Promise<FileNode[]> {
	const key = getTreeCacheKey(workspacePath, path, depth);
	const existing = treeInFlight.get(key);
	if (existing) return existing;

	const request = fetchFileTreeMux(workspacePath, path, depth, false)
		.then((result) => {
			return result;
		})
		.finally(() => {
			treeInFlight.delete(key);
		});

	treeInFlight.set(key, request);
	return request;
}

/** Lazy-load children for a directory node, merging into the existing tree. */
function mergeChildrenIntoTree(
	tree: FileNode[],
	parentPath: string,
	children: FileNode[],
): FileNode[] {
	return tree.map((node) => {
		if (node.path === parentPath) {
			return { ...node, children };
		}
		if (node.children && parentPath.startsWith(`${node.path}/`)) {
			return {
				...node,
				children: mergeChildrenIntoTree(node.children, parentPath, children),
			};
		}
		return node;
	});
}

/** Check if a directory node needs lazy-loading.
 *  A directory needs loading if:
 *  - It has no children array at all (depth boundary - never fetched)
 *  - Its children contain subdirectories without their own children arrays
 */
function needsLazyLoad(node: FileNode): boolean {
	if (node.type !== "directory") return false;
	// No children array at all - this directory was at the depth boundary
	if (node.children === undefined) return true;
	if (node.children.length === 0) return false;
	// Children exist but some subdirectories haven't been expanded yet
	return node.children.some(
		(child) => child.type === "directory" && child.children === undefined,
	);
}

// File extensions that can be previewed
const PREVIEWABLE_EXTENSIONS = new Set([
	".txt",
	".md",
	".json",
	".xml",
	".yaml",
	".yml",
	".toml",
	".js",
	".ts",
	".jsx",
	".tsx",
	".css",
	".scss",
	".html",
	".py",
	".rb",
	".go",
	".rs",
	".java",
	".c",
	".cpp",
	".h",
	".sh",
	".bash",
	".zsh",
	".fish",
	".sql",
	".graphql",
	".env",
	".gitignore",
	".dockerignore",
	".config",
	".conf",
	".ini",
	".cfg",
	".log",
	// Images
	".png",
	".jpg",
	".jpeg",
	".gif",
	".webp",
	".svg",
	".bmp",
	".ico",
	// Videos
	".mp4",
	".webm",
	".ogg",
	".ogv",
	".mov",
	".avi",
	".mkv",
	".m4v",
	// Audio
	".mp3",
	".wav",
	".flac",
	".aac",
	".m4a",
	".opus",
	// Documents
	".pdf",
	".typ",
]);

function isPreviewable(filename: string): boolean {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
	return PREVIEWABLE_EXTENSIONS.has(ext) || !filename.includes(".");
}

// Image extensions that can be opened in canvas
const IMAGE_EXTENSIONS = new Set([
	".png",
	".jpg",
	".jpeg",
	".gif",
	".webp",
	".svg",
	".bmp",
]);

function isImageFile(filename: string): boolean {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
	return IMAGE_EXTENSIONS.has(ext);
}

function formatFileSize(bytes?: number): string {
	if (bytes === undefined) return "-";
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(timestamp?: number): string {
	if (!timestamp) return "-";
	const date = new Date(timestamp * 1000);
	return date.toLocaleDateString("de-DE", {
		day: "2-digit",
		month: "2-digit",
		year: "numeric",
	});
}

export type ViewMode = "tree" | "list" | "grid";

export interface FileTreeState {
	currentPath: string;
	expanded: Record<string, boolean>;
	selectedFile: string | null;
	selectedFiles: Set<string>;
	viewMode: ViewMode;
}

export const initialFileTreeState: FileTreeState = {
	currentPath: ".",
	expanded: {},
	selectedFile: null,
	selectedFiles: new Set(),
	viewMode: "tree",
};

interface FileTreeViewProps {
	onPreviewFile?: (filePath: string) => void;
	onOpenInCanvas?: (filePath: string) => void;
	workspacePath?: string | null;
	/** External state for persistence across view switches */
	state?: FileTreeState;
	/** Callback to update external state */
	onStateChange?: (state: FileTreeState) => void;
}

export function FileTreeView({
	onPreviewFile,
	onOpenInCanvas,
	workspacePath,
	state,
	onStateChange,
}: FileTreeViewProps) {
	const normalizedWorkspacePath = useMemo(
		() => normalizeWorkspacePath(workspacePath ?? null),
		[workspacePath],
	);
	const [tree, setTree] = useState<FileNode[]>([]);
	const [error, setError] = useState<string>("");
	const [loading, setLoading] = useState(false);
	const [uploading, setUploading] = useState(false);
	const [newFolderName, setNewFolderName] = useState<string | null>(null);
	const [renamingPath, setRenamingPath] = useState<string | null>(null);
	const [loadingDirs, setLoadingDirs] = useState<Set<string>>(new Set());
	const [pathDialog, setPathDialog] = useState<{
		title: string;
		sourcePath: string;
		onConfirm: (value: string) => void;
	} | null>(null);
	const fileInputRef = useRef<HTMLInputElement>(null);
	const lastLoadRef = useRef<{ key: string; ts: number } | null>(null);

	// Use external state if provided, otherwise use internal state
	const [internalExpanded, setInternalExpanded] = useState<
		Record<string, boolean>
	>({});
	const [internalSelectedFile, setInternalSelectedFile] = useState<
		string | null
	>(null);
	const [internalSelectedFiles, setInternalSelectedFiles] = useState<
		Set<string>
	>(new Set());
	const [internalViewMode, setInternalViewMode] = useState<ViewMode>("tree");
	const [internalCurrentPath, setInternalCurrentPath] = useState<string>(".");

	const expanded = state?.expanded ?? internalExpanded;
	const selectedFile = state?.selectedFile ?? internalSelectedFile;
	const selectedFiles = state?.selectedFiles ?? internalSelectedFiles;
	const viewMode = state?.viewMode ?? internalViewMode;
	const currentPath = state?.currentPath ?? internalCurrentPath;

	// Use a ref for external state to avoid recreating updateState on every render.
	// This breaks the dependency chain that caused loadTree to re-run on every
	// state change, which wiped out lazy-loaded children with stale cache data.
	const stateRef = useRef(state);
	stateRef.current = state;

	const updateState = useCallback(
		(updates: Partial<FileTreeState>) => {
			if (onStateChange && stateRef.current) {
				onStateChange({ ...stateRef.current, ...updates });
			} else {
				if (updates.expanded !== undefined)
					setInternalExpanded(updates.expanded);
				if (updates.selectedFile !== undefined)
					setInternalSelectedFile(updates.selectedFile);
				if (updates.selectedFiles !== undefined)
					setInternalSelectedFiles(updates.selectedFiles);
				if (updates.viewMode !== undefined)
					setInternalViewMode(updates.viewMode);
				if (updates.currentPath !== undefined)
					setInternalCurrentPath(updates.currentPath);
			}
		},
		[onStateChange],
	);

	const cacheKey = normalizedWorkspacePath ?? null;

	const loadTree = useCallback(
		async (path: string, preserveState = false, skipCache = false, silent = false) => {
			if (!normalizedWorkspacePath || !cacheKey) return;
			const requestKey = getTreeCacheKey(cacheKey, path, INITIAL_DEPTH);
			const now = Date.now();
			if (
				lastLoadRef.current &&
				lastLoadRef.current.key === requestKey &&
				now - lastLoadRef.current.ts < 600
			) {
				return;
			}
			lastLoadRef.current = { key: requestKey, ts: now };

			// Check cache first (unless explicitly skipping)
			if (!skipCache) {
				const cached = getCachedTree(cacheKey, path, INITIAL_DEPTH);
				if (cached) {
					setTree(cached);
					if (!preserveState) {
						updateState({ currentPath: path });
					}
					setLoading(false);
					return;
				}
			}

			if (!silent) {
				setLoading(true);
				setError("");
			}
			try {
				const data = await fetchFileTree(
					normalizedWorkspacePath,
					path,
					INITIAL_DEPTH,
				);
				// Cache the result
				setCachedTree(cacheKey, path, data, INITIAL_DEPTH);
				setTree(data);
				if (!preserveState) {
					updateState({ currentPath: path });
				}
				if (!silent) setError("");
			} catch (err) {
				if (!silent) {
					setError(
						err instanceof Error ? err.message : "Unable to load file tree",
					);
				}
			} finally {
				if (!silent) setLoading(false);
			}
		},
		[normalizedWorkspacePath, updateState, cacheKey],
	);

	/** Lazy-load deeper children for a directory when it is expanded. */
	const lazyLoadChildren = useCallback(
		async (dirPath: string) => {
			if (!normalizedWorkspacePath || !cacheKey) return;

			// Check cache for this sub-tree
			const cached = getCachedTree(cacheKey, dirPath, LAZY_LOAD_DEPTH);
			if (cached) {
				setTree((prev) => mergeChildrenIntoTree(prev, dirPath, cached));
				return;
			}

			setLoadingDirs((prev) => new Set(prev).add(dirPath));
			try {
				const children = await fetchFileTree(
					normalizedWorkspacePath,
					dirPath,
					LAZY_LOAD_DEPTH,
				);
				setCachedTree(cacheKey, dirPath, children, LAZY_LOAD_DEPTH);
				setTree((prev) => mergeChildrenIntoTree(prev, dirPath, children));
			} catch {
				// Silently fail - the user can still see the shallow children
			} finally {
				setLoadingDirs((prev) => {
					const next = new Set(prev);
					next.delete(dirPath);
					return next;
				});
			}
		},
		[normalizedWorkspacePath, cacheKey],
	);

	const refreshTree = useCallback(() => {
		// Force refresh bypasses cache
		loadTree(currentPath, true, true);
	}, [loadTree, currentPath]);

	// Only re-load the tree when the workspace or current path actually changes,
	// not when loadTree is recreated due to dependency churn. This prevents
	// lazy-loaded children from being wiped out by stale shallow cache data.
	const loadTreeRef = useRef(loadTree);
	loadTreeRef.current = loadTree;
	useEffect(() => {
		if (!normalizedWorkspacePath) return;
		loadTreeRef.current(currentPath, true);
	}, [currentPath, normalizedWorkspacePath]);

	// Auto-refresh the file tree periodically so new files created by agents
	// appear without manual interaction. Uses a 5s interval, skips if the
	// tab is hidden or a load is already in progress.
	useEffect(() => {
		if (!normalizedWorkspacePath) return;
		const interval = setInterval(() => {
			if (document.hidden) return;
			// preserveState=true: keep expanded dirs
			// skipCache=true: force fresh data from backend
			// silent=true: no loading spinner or error flash
			loadTreeRef.current(currentPath, true, true, true);
		}, 5000);
		return () => clearInterval(interval);
	}, [normalizedWorkspacePath, currentPath]);

	/** Find a node in the tree by path. */
	const findNode = useCallback(
		(nodes: FileNode[], path: string): FileNode | null => {
			for (const node of nodes) {
				if (node.path === path) return node;
				if (node.children) {
					const found = findNode(node.children, path);
					if (found) return found;
				}
			}
			return null;
		},
		[],
	);

	const toggle = useCallback(
		(path: string) => {
			const willExpand = !expanded[path];
			updateState({ expanded: { ...expanded, [path]: willExpand } });

			// Lazy-load deeper children when expanding a directory
			if (willExpand) {
				const node = findNode(tree, path);
				if (node && needsLazyLoad(node)) {
					void lazyLoadChildren(path);
				}
			}
		},
		[expanded, updateState, tree, findNode, lazyLoadChildren],
	);

	const handleSelectFile = (
		path: string,
		name: string,
		isDirectory: boolean,
		event?: React.MouseEvent,
	) => {
		const isShiftClick = event?.shiftKey;

		if (isShiftClick) {
			// Shift+click: toggle selection (multi-select)
			const newSelection = new Set(selectedFiles);
			if (newSelection.has(path)) {
				newSelection.delete(path);
			} else {
				newSelection.add(path);
			}
			updateState({ selectedFiles: newSelection, selectedFile: path });
		} else {
			// Normal click: clear selection, preview file if previewable
			updateState({ selectedFile: path, selectedFiles: new Set() });
			if (!isDirectory && onPreviewFile && isPreviewable(name)) {
				onPreviewFile(path);
			}
		}
	};

	const handleNavigateToFolder = (path: string) => {
		updateState({ currentPath: path, expanded: {}, selectedFiles: new Set() });
	};

	const handleGoUp = () => {
		if (currentPath === ".") return;
		const parts = currentPath.split("/");
		parts.pop();
		const parentPath = parts.length === 0 ? "." : parts.join("/");
		handleNavigateToFolder(parentPath);
	};

	const handleGoHome = () => {
		handleNavigateToFolder(".");
	};

	const setViewMode = (mode: ViewMode) => {
		updateState({ viewMode: mode });
	};

	const handleUploadClick = () => {
		fileInputRef.current?.click();
	};

	const handleFileChange = async (
		event: React.ChangeEvent<HTMLInputElement>,
	) => {
		const files = event.target.files;
		if (!files || files.length === 0 || !normalizedWorkspacePath || !cacheKey)
			return;

		setUploading(true);
		setError("");

		try {
			for (const file of Array.from(files)) {
				const destPath =
					currentPath === "." ? file.name : `${currentPath}/${file.name}`;
				await uploadFileMux(normalizedWorkspacePath, destPath, file);
			}
			await refreshTree();
		} catch (err) {
			setError(err instanceof Error ? err.message : "Upload failed");
		} finally {
			setUploading(false);
			// Reset input
			if (fileInputRef.current) {
				fileInputRef.current.value = "";
			}
		}
	};

	const handleDownload = (path: string, isDirectory: boolean) => {
		if (!normalizedWorkspacePath || !cacheKey) return;
		if (isDirectory) {
			setError("Directory download is not supported yet.");
			return;
		}
		void downloadFileMux(normalizedWorkspacePath, path);
	};

	const handleDownloadSelected = () => {
		if (!normalizedWorkspacePath || !cacheKey || selectedFiles.size === 0)
			return;

		if (selectedFiles.size === 1) {
			const path = Array.from(selectedFiles)[0];
			handleDownload(path, false); // We don't know if it's a directory
		} else {
			setError("Multi-file download is not supported yet.");
		}
	};

	const handleDelete = async (path: string) => {
		if (!normalizedWorkspacePath || !cacheKey) return;

		try {
			await deletePathMux(normalizedWorkspacePath, path, true);
			await refreshTree();
			// Clear selection if deleted file was selected
			if (selectedFiles.has(path)) {
				const newSelection = new Set(selectedFiles);
				newSelection.delete(path);
				updateState({ selectedFiles: newSelection });
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : "Delete failed");
		}
	};

	const handleDeleteSelected = async () => {
		if (!normalizedWorkspacePath || !cacheKey || selectedFiles.size === 0)
			return;

		try {
			for (const path of selectedFiles) {
				await deletePathMux(normalizedWorkspacePath, path, true);
			}
			await refreshTree();
			updateState({ selectedFiles: new Set() });
		} catch (err) {
			setError(err instanceof Error ? err.message : "Delete failed");
		}
	};

	const handleCopy = (path: string) => {
		if (!normalizedWorkspacePath || !cacheKey) return;
		setPathDialog({
			title: "Copy to",
			sourcePath: path,
			onConfirm: async (target: string) => {
				if (!target || target === path) return;
				try {
					await copyPathMux(normalizedWorkspacePath, path, target, false);
					await refreshTree();
				} catch (err) {
					setError(err instanceof Error ? err.message : "Copy failed");
				}
			},
		});
	};

	const handleMove = (path: string) => {
		if (!normalizedWorkspacePath || !cacheKey) return;
		setPathDialog({
			title: "Move to",
			sourcePath: path,
			onConfirm: async (target: string) => {
				if (!target || target === path) return;
				try {
					await movePathMux(normalizedWorkspacePath, path, target, false);
					await refreshTree();
				} catch (err) {
					setError(err instanceof Error ? err.message : "Move failed");
				}
			},
		});
	};

	const handleNewFolder = () => {
		setNewFolderName("");
	};

	const handleCreateFolderWithName = useCallback(
		async (name: string) => {
			if (!normalizedWorkspacePath || !cacheKey || !name) {
				setNewFolderName(null);
				return;
			}

			try {
				const folderPath =
					currentPath === "." ? name : `${currentPath}/${name}`;
				await createDirectoryMux(normalizedWorkspacePath, folderPath, true);
				await refreshTree();
			} catch (err) {
				setError(err instanceof Error ? err.message : "Create folder failed");
			} finally {
				setNewFolderName(null);
			}
		},
		[normalizedWorkspacePath, cacheKey, currentPath, refreshTree],
	);

	const handleStartRename = (path: string, _currentName: string) => {
		setRenamingPath(path);
	};

	const handleCancelRename = useCallback(() => {
		setRenamingPath(null);
	}, []);

	const handleConfirmRename = useCallback(
		async (newName: string) => {
			if (!normalizedWorkspacePath || !cacheKey || !renamingPath || !newName) {
				setRenamingPath(null);
				return;
			}

			const oldName = renamingPath.split("/").pop();

			// Skip if name unchanged
			if (newName === oldName) {
				setRenamingPath(null);
				return;
			}

			try {
				// Build new path by replacing the last segment
				const pathParts = renamingPath.split("/");
				pathParts[pathParts.length - 1] = newName;
				const newPath = pathParts.join("/");

				await renamePathMux(normalizedWorkspacePath, renamingPath, newPath);
				await refreshTree();
			} catch (err) {
				setError(err instanceof Error ? err.message : "Rename failed");
			} finally {
				setRenamingPath(null);
			}
		},
		[normalizedWorkspacePath, cacheKey, renamingPath, refreshTree],
	);

	const clearSelection = () => {
		updateState({ selectedFiles: new Set(), selectedFile: null });
	};

	// Get breadcrumb parts from current path
	const getBreadcrumbs = () => {
		if (currentPath === ".") return [{ name: "Home", path: "." }];
		const parts = currentPath.split("/");
		const breadcrumbs = [{ name: "Home", path: "." }];
		let accumulated = "";
		for (const part of parts) {
			accumulated = accumulated ? `${accumulated}/${part}` : part;
			breadcrumbs.push({ name: part, path: accumulated });
		}
		return breadcrumbs;
	};

	// For default chat, we always have access; for workspace, need workspace path
	if (!normalizedWorkspacePath) {
		return (
			<div className="h-full flex items-center justify-center p-4 text-sm text-muted-foreground">
				Select a chat to browse files.
			</div>
		);
	}

	if (loading && tree.length === 0) {
		return (
			<div className="p-4 text-sm text-muted-foreground">
				Loading workspace tree...
			</div>
		);
	}

	if (error && tree.length === 0) {
		return <div className="p-4 text-sm text-destructive">{error}</div>;
	}

	const breadcrumbs = getBreadcrumbs();
	const hasSelection = selectedFiles.size > 0;

	return (
		<div className="h-full flex flex-col overflow-hidden">
			{/* Hidden file input */}
			<input
				ref={fileInputRef}
				type="file"
				multiple
				className="hidden"
				onChange={handleFileChange}
			/>

			{/* Navigation bar */}
			<div className="flex-shrink-0 flex items-center gap-1 p-2 border-b border-border">
				<button
					type="button"
					onClick={handleGoHome}
					disabled={currentPath === "."}
					className={cn(
						"p-1.5 rounded transition-colors",
						currentPath === "."
							? "text-muted-foreground/50 cursor-not-allowed"
							: "text-muted-foreground hover:text-foreground hover:bg-muted",
					)}
					title="Go to root"
				>
					<Home className="w-4 h-4" />
				</button>
				<button
					type="button"
					onClick={handleGoUp}
					disabled={currentPath === "."}
					className={cn(
						"p-1.5 rounded transition-colors",
						currentPath === "."
							? "text-muted-foreground/50 cursor-not-allowed"
							: "text-muted-foreground hover:text-foreground hover:bg-muted",
					)}
					title="Go up"
				>
					<FolderUp className="w-4 h-4" />
				</button>

				{/* Breadcrumbs */}
				<div className="flex-1 flex items-center gap-1 overflow-x-auto scrollbar-none [scrollbar-width:none] [-ms-overflow-style:none] [&::-webkit-scrollbar]:hidden text-sm ml-2">
					{breadcrumbs.map((crumb, index) => (
						<span
							key={crumb.path}
							className="flex items-center gap-1 whitespace-nowrap"
						>
							{index > 0 && (
								<ChevronRight className="w-3 h-3 text-muted-foreground" />
							)}
							<button
								type="button"
								onClick={() => handleNavigateToFolder(crumb.path)}
								className={cn(
									"hover:text-primary transition-colors",
									index === breadcrumbs.length - 1
										? "text-foreground font-medium"
										: "text-muted-foreground",
								)}
							>
								{crumb.name}
							</button>
						</span>
					))}
				</div>

				{/* Actions */}
				<div className="flex items-center gap-1 ml-2">
					<button
						type="button"
						onClick={handleUploadClick}
						disabled={uploading}
						className="p-1.5 rounded transition-colors text-muted-foreground hover:text-foreground hover:bg-muted"
						title="Upload files"
					>
						{uploading ? (
							<Loader2 className="w-4 h-4 animate-spin" />
						) : (
							<Upload className="w-4 h-4" />
						)}
					</button>
					<button
						type="button"
						onClick={handleNewFolder}
						className="p-1.5 rounded transition-colors text-muted-foreground hover:text-foreground hover:bg-muted"
						title="New folder"
					>
						<FolderPlus className="w-4 h-4" />
					</button>
					{hasSelection && (
						<>
							<button
								type="button"
								onClick={handleDownloadSelected}
								className="p-1.5 rounded transition-colors text-muted-foreground hover:text-foreground hover:bg-muted"
								title={`Download ${selectedFiles.size} item(s)`}
							>
								<Download className="w-4 h-4" />
							</button>
							<button
								type="button"
								onClick={handleDeleteSelected}
								className="p-1.5 rounded transition-colors text-muted-foreground hover:text-destructive hover:bg-muted"
								title={`Delete ${selectedFiles.size} item(s)`}
							>
								<Trash2 className="w-4 h-4" />
							</button>
						</>
					)}
				</div>

				{/* View mode toggle */}
				<div className="flex items-center gap-1 ml-2 border-l border-border pl-2">
					<button
						type="button"
						onClick={() => setViewMode("tree")}
						className={cn(
							"p-1.5 rounded transition-colors",
							viewMode === "tree"
								? "bg-primary/20 text-primary"
								: "text-muted-foreground hover:text-foreground hover:bg-muted",
						)}
						title="Tree view"
					>
						<Folder className="w-4 h-4" />
					</button>
					<button
						type="button"
						onClick={() => setViewMode("list")}
						className={cn(
							"p-1.5 rounded transition-colors",
							viewMode === "list"
								? "bg-primary/20 text-primary"
								: "text-muted-foreground hover:text-foreground hover:bg-muted",
						)}
						title="List view"
					>
						<List className="w-4 h-4" />
					</button>
					<button
						type="button"
						onClick={() => setViewMode("grid")}
						className={cn(
							"p-1.5 rounded transition-colors",
							viewMode === "grid"
								? "bg-primary/20 text-primary"
								: "text-muted-foreground hover:text-foreground hover:bg-muted",
						)}
						title="Grid view"
					>
						<LayoutGrid className="w-4 h-4" />
					</button>
				</div>
			</div>

			{/* Selection info bar */}
			{hasSelection && (
				<div className="flex-shrink-0 flex items-center justify-between px-3 py-1.5 bg-primary/10 border-b border-border text-xs">
					<span>{selectedFiles.size} item(s) selected</span>
					<button
						type="button"
						onClick={clearSelection}
						className="text-muted-foreground hover:text-foreground"
					>
						Clear selection
					</button>
				</div>
			)}

			{/* Error message */}
			{error && (
				<div className="flex-shrink-0 px-3 py-2 bg-destructive/10 text-destructive text-xs">
					{error}
				</div>
			)}

			{/* New folder input */}
			{newFolderName !== null && (
				<NewFolderInput
					onConfirm={handleCreateFolderWithName}
					onCancel={() => setNewFolderName(null)}
				/>
			)}

			{/* File content */}
			<div
				className="flex-1 overflow-auto"
				onMouseDown={(e) => {
					// Clear selection when clicking empty space
					if (e.target === e.currentTarget) {
						clearSelection();
					}
				}}
			>
				{tree.length === 0 ? (
					<div className="text-sm text-muted-foreground p-4">
						No files found.
					</div>
				) : viewMode === "tree" ? (
					<TreeView
						nodes={tree}
						expanded={expanded}
						onToggle={toggle}
						selectedFiles={selectedFiles}
						onSelectFile={handleSelectFile}
						onNavigateToFolder={handleNavigateToFolder}
						onDownload={handleDownload}
						onDelete={handleDelete}
						onRename={handleStartRename}
						onCopy={handleCopy}
						onMove={handleMove}
						renamingPath={renamingPath}
						onRenameConfirm={handleConfirmRename}
						onRenameCancel={handleCancelRename}
						onOpenInCanvas={onOpenInCanvas}
						loadingDirs={loadingDirs}
					/>
				) : viewMode === "list" ? (
					<ListView
						files={tree}
						selectedFiles={selectedFiles}
						onSelectFile={handleSelectFile}
						onNavigateToFolder={handleNavigateToFolder}
						onDownload={handleDownload}
						onDelete={handleDelete}
						onRename={handleStartRename}
						onCopy={handleCopy}
						onMove={handleMove}
						renamingPath={renamingPath}
						onRenameConfirm={handleConfirmRename}
						onRenameCancel={handleCancelRename}
						onOpenInCanvas={onOpenInCanvas}
					/>
				) : (
					<GridView
						files={tree}
						selectedFiles={selectedFiles}
						onSelectFile={handleSelectFile}
						onNavigateToFolder={handleNavigateToFolder}
						onDownload={handleDownload}
						onDelete={handleDelete}
						onRename={handleStartRename}
						onCopy={handleCopy}
						onMove={handleMove}
						renamingPath={renamingPath}
						onRenameConfirm={handleConfirmRename}
						onRenameCancel={handleCancelRename}
						onOpenInCanvas={onOpenInCanvas}
					/>
				)}
			</div>

			{/* Destination picker dialog for copy/move operations */}
			<DestinationPickerDialog
				open={pathDialog !== null}
				title={pathDialog?.title ?? ""}
				sourcePath={pathDialog?.sourcePath ?? ""}
				tree={tree}
				onConfirm={(value) => {
					pathDialog?.onConfirm(value);
					setPathDialog(null);
				}}
				onCancel={() => setPathDialog(null)}
			/>
		</div>
	);
}

// Destination picker dialog for copy/move with directory tree
const DestinationPickerDialog = memo(function DestinationPickerDialog({
	open,
	title,
	sourcePath,
	tree,
	onConfirm,
	onCancel,
}: {
	open: boolean;
	title: string;
	sourcePath: string;
	tree: FileNode[];
	onConfirm: (value: string) => void;
	onCancel: () => void;
}) {
	const [customPath, setCustomPath] = useState("");
	const [selectedDir, setSelectedDir] = useState<string | null>(null);
	const [expanded, setExpanded] = useState<Set<string>>(new Set());
	const inputRef = useRef<HTMLInputElement>(null);

	const fileName = sourcePath.split("/").pop() ?? sourcePath;
	const sourceDir = sourcePath.includes("/")
		? sourcePath.substring(0, sourcePath.lastIndexOf("/"))
		: ".";

	useEffect(() => {
		if (open) {
			setCustomPath("");
			setSelectedDir(null);
			// Auto-expand the source directory's parents
			const parts = sourcePath.split("/");
			const parentPaths = new Set<string>();
			for (let i = 1; i < parts.length; i++) {
				parentPaths.add(parts.slice(0, i).join("/"));
			}
			setExpanded(parentPaths);
		}
	}, [open, sourcePath]);

	const handleConfirm = () => {
		if (customPath.trim()) {
			onConfirm(customPath.trim());
		} else if (selectedDir !== null) {
			// Combine selected directory with the filename
			const dest =
				selectedDir === "." ? fileName : `${selectedDir}/${fileName}`;
			onConfirm(dest);
		}
	};

	const toggleExpand = (path: string) => {
		setExpanded((prev) => {
			const next = new Set(prev);
			if (next.has(path)) {
				next.delete(path);
			} else {
				next.add(path);
			}
			return next;
		});
	};

	const renderDirNode = (node: FileNode, depth: number) => {
		if (node.type !== "directory") return null;

		// Don't show source dir's parent as a target if it's the same path
		const isSourceDir = node.path === sourceDir;
		const isSelected = selectedDir === node.path;
		const isExpanded = expanded.has(node.path);
		const hasChildren = node.children?.some((c) => c.type === "directory");

		return (
			<div key={node.path}>
				<button
					type="button"
					onClick={() => {
						setSelectedDir(node.path);
						setCustomPath("");
					}}
					onDoubleClick={() => toggleExpand(node.path)}
					className={cn(
						"flex items-center gap-1.5 w-full px-2 py-1 text-sm rounded transition-colors text-left",
						isSelected
							? "bg-primary/15 text-primary"
							: "hover:bg-muted text-foreground",
						isSourceDir && "text-muted-foreground",
					)}
					style={{ paddingLeft: `${depth * 16 + 8}px` }}
				>
					{hasChildren ? (
						<button
							type="button"
							onClick={(e) => {
								e.stopPropagation();
								toggleExpand(node.path);
							}}
							className="p-0 shrink-0"
						>
							{isExpanded ? (
								<ChevronDown className="w-3.5 h-3.5 text-muted-foreground" />
							) : (
								<ChevronRight className="w-3.5 h-3.5 text-muted-foreground" />
							)}
						</button>
					) : (
						<span className="w-3.5" />
					)}
					<Folder className="w-4 h-4 shrink-0 text-muted-foreground" />
					<span className="truncate">{node.name}</span>
					{isSourceDir && (
						<span className="text-xs text-muted-foreground ml-auto">
							current
						</span>
					)}
				</button>
				{isExpanded &&
					node.children
						?.filter((c) => c.type === "directory")
						.map((child) => renderDirNode(child, depth + 1))}
			</div>
		);
	};

	return (
		<Dialog open={open} onOpenChange={(o) => !o && onCancel()}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>{title}</DialogTitle>
					<DialogDescription>
						Select a destination for{" "}
						<span className="font-medium text-foreground">{fileName}</span>
					</DialogDescription>
				</DialogHeader>

				{/* Directory tree */}
				<div className="max-h-64 overflow-y-auto rounded-md border border-border bg-muted/30 py-1">
					{/* Root directory option */}
					<button
						type="button"
						onClick={() => {
							setSelectedDir(".");
							setCustomPath("");
						}}
						className={cn(
							"flex items-center gap-1.5 w-full px-2 py-1 text-sm rounded transition-colors text-left",
							selectedDir === "."
								? "bg-primary/15 text-primary"
								: "hover:bg-muted text-foreground",
						)}
						style={{ paddingLeft: "8px" }}
					>
						<span className="w-3.5" />
						<Folder className="w-4 h-4 shrink-0 text-muted-foreground" />
						<span className="truncate">/ (root)</span>
						{sourceDir === "." && (
							<span className="text-xs text-muted-foreground ml-auto">
								current
							</span>
						)}
					</button>
					{tree
						.filter((n) => n.type === "directory")
						.map((node) => renderDirNode(node, 1))}
				</div>

				{/* Custom path input as alternative */}
				<div className="flex items-center gap-2">
					<span className="text-xs text-muted-foreground whitespace-nowrap">
						Or type path:
					</span>
					<input
						ref={inputRef}
						type="text"
						value={customPath}
						onChange={(e) => {
							setCustomPath(e.target.value);
							if (e.target.value) setSelectedDir(null);
						}}
						placeholder={sourcePath}
						className="flex-1 rounded-md border border-border bg-background px-2.5 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
						onKeyDown={(e) => {
							if (e.key === "Escape") {
								e.preventDefault();
								onCancel();
							} else if (e.key === "Enter") {
								e.preventDefault();
								handleConfirm();
							}
						}}
					/>
				</div>

				<DialogFooter>
					<button
						type="button"
						onClick={onCancel}
						className="inline-flex items-center justify-center rounded-md px-4 py-2 text-sm font-medium border border-border bg-background text-foreground hover:bg-accent hover:text-accent-foreground transition-colors"
					>
						Cancel
					</button>
					<button
						type="button"
						onClick={handleConfirm}
						disabled={!selectedDir && !customPath.trim()}
						className="inline-flex items-center justify-center rounded-md px-4 py-2 text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:pointer-events-none"
					>
						Confirm
					</button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
});

// Isolated rename input component - manages its own state to avoid parent re-renders
const RenameInput = memo(function RenameInput({
	initialValue,
	onConfirm,
	onCancel,
}: {
	initialValue: string;
	onConfirm: (newValue: string) => void;
	onCancel: () => void;
}) {
	const [value, setValue] = useState(initialValue);
	const inputRef = useRef<HTMLInputElement>(null);

	useEffect(() => {
		// Focus and select all text after mount
		if (inputRef.current) {
			inputRef.current.focus();
			inputRef.current.select();
		}
	}, []);

	const handleConfirm = useCallback(() => {
		onConfirm(value);
	}, [value, onConfirm]);

	return (
		<input
			ref={inputRef}
			type="text"
			value={value}
			onChange={(e) => setValue(e.target.value)}
			onKeyDown={(e) => {
				e.stopPropagation();
				if (e.key === "Enter") handleConfirm();
				if (e.key === "Escape") onCancel();
			}}
			onBlur={handleConfirm}
			className="flex-1 min-w-0 bg-background border border-input rounded px-1 text-sm text-foreground"
			onClick={(e) => e.stopPropagation()}
		/>
	);
});

// Isolated new folder input component - manages its own state to avoid parent re-renders
const NewFolderInput = memo(function NewFolderInput({
	onConfirm,
	onCancel,
}: {
	onConfirm: (name: string) => void;
	onCancel: () => void;
}) {
	const [value, setValue] = useState("");
	const inputRef = useRef<HTMLInputElement>(null);

	useEffect(() => {
		if (inputRef.current) {
			inputRef.current.focus();
		}
	}, []);

	const handleConfirm = useCallback(() => {
		if (value.trim()) {
			onConfirm(value.trim());
		} else {
			onCancel();
		}
	}, [value, onConfirm, onCancel]);

	return (
		<div className="flex-shrink-0 flex items-center gap-2 px-3 py-2 border-b border-border bg-muted/30">
			<FolderPlus className="w-4 h-4 text-muted-foreground" />
			<input
				ref={inputRef}
				type="text"
				value={value}
				onChange={(e) => setValue(e.target.value)}
				onKeyDown={(e) => {
					if (e.key === "Enter") handleConfirm();
					if (e.key === "Escape") onCancel();
				}}
				onBlur={handleConfirm}
				placeholder="New folder name..."
				className="flex-1 bg-transparent border-none outline-none text-sm"
			/>
		</div>
	);
});

// Context menu wrapper for file items
function FileContextMenu({
	children,
	node,
	onDownload,
	onDelete,
	onRename,
	onCopy,
	onMove,
	onOpenInCanvas,
}: {
	children: React.ReactNode;
	node: FileNode;
	onDownload: (path: string, isDirectory: boolean) => void;
	onDelete: (path: string) => void;
	onRename: (path: string, currentName: string) => void;
	onCopy: (path: string) => void;
	onMove: (path: string) => void;
	onOpenInCanvas?: (path: string) => void;
}) {
	const isImage = node.type === "file" && isImageFile(node.name);

	return (
		<ContextMenu>
			<ContextMenuTrigger className="contents">{children}</ContextMenuTrigger>
			<ContextMenuContent>
				{isImage && onOpenInCanvas && (
					<>
						<ContextMenuItem onClick={() => onOpenInCanvas(node.path)}>
							<PaintBucket className="w-4 h-4 mr-2" />
							Open in Canvas
						</ContextMenuItem>
						<ContextMenuSeparator />
					</>
				)}
				<ContextMenuItem onClick={() => onRename(node.path, node.name)}>
					<Pencil className="w-4 h-4 mr-2" />
					Rename
				</ContextMenuItem>
				<ContextMenuItem onClick={() => onCopy(node.path)}>
					<Copy className="w-4 h-4 mr-2" />
					Copy
				</ContextMenuItem>
				<ContextMenuItem onClick={() => onMove(node.path)}>
					<MoveRight className="w-4 h-4 mr-2" />
					Move
				</ContextMenuItem>
				<ContextMenuItem
					onClick={() => onDownload(node.path, node.type === "directory")}
				>
					<Download className="w-4 h-4 mr-2" />
					{node.type === "directory" ? "Download as ZIP" : "Download"}
				</ContextMenuItem>
				<ContextMenuSeparator />
				<ContextMenuItem
					onClick={() => onDelete(node.path)}
					className="text-destructive focus:text-destructive"
				>
					<Trash2 className="w-4 h-4 mr-2" />
					Delete
				</ContextMenuItem>
			</ContextMenuContent>
		</ContextMenu>
	);
}

// Tree View Component
function TreeView({
	nodes,
	expanded,
	onToggle,
	selectedFiles,
	onSelectFile,
	onNavigateToFolder,
	onDownload,
	onDelete,
	onRename,
	onCopy,
	onMove,
	renamingPath,
	onRenameConfirm,
	onRenameCancel,
	onOpenInCanvas,
	loadingDirs,
}: {
	nodes: FileNode[];
	expanded: Record<string, boolean>;
	onToggle: (path: string) => void;
	selectedFiles: Set<string>;
	onSelectFile: (
		path: string,
		name: string,
		isDirectory: boolean,
		event?: React.MouseEvent,
	) => void;
	onNavigateToFolder: (path: string) => void;
	onDownload: (path: string, isDirectory: boolean) => void;
	onDelete: (path: string) => void;
	onRename: (path: string, currentName: string) => void;
	onCopy: (path: string) => void;
	onMove: (path: string) => void;
	renamingPath: string | null;
	onRenameConfirm: (newName: string) => void;
	onRenameCancel: () => void;
	onOpenInCanvas?: (path: string) => void;
	loadingDirs?: Set<string>;
}) {
	// Sort: directories first, then files, both alphabetically
	const sortedNodes = [...nodes].sort((a, b) => {
		if (a.type === "directory" && b.type !== "directory") return -1;
		if (a.type !== "directory" && b.type === "directory") return 1;
		return a.name.localeCompare(b.name);
	});

	return (
		<ul className="py-1">
			{sortedNodes.map((node) => (
				<TreeRow
					key={node.path}
					node={node}
					level={0}
					expanded={expanded}
					onToggle={onToggle}
					onSelectFile={onSelectFile}
					selectedFiles={selectedFiles}
					onNavigateToFolder={onNavigateToFolder}
					onDownload={onDownload}
					onDelete={onDelete}
					onRename={onRename}
					onCopy={onCopy}
					onMove={onMove}
					renamingPath={renamingPath}
					onRenameConfirm={onRenameConfirm}
					onRenameCancel={onRenameCancel}
					onOpenInCanvas={onOpenInCanvas}
					loadingDirs={loadingDirs}
				/>
			))}
		</ul>
	);
}

// Tree Row Component
function TreeRow({
	node,
	level,
	expanded,
	onToggle,
	onSelectFile,
	selectedFiles,
	onNavigateToFolder,
	onDownload,
	onDelete,
	onRename,
	onCopy,
	onMove,
	renamingPath,
	onRenameConfirm,
	onRenameCancel,
	onOpenInCanvas,
	loadingDirs,
}: {
	node: FileNode;
	level: number;
	expanded: Record<string, boolean>;
	onToggle: (path: string) => void;
	onSelectFile: (
		path: string,
		name: string,
		isDirectory: boolean,
		event?: React.MouseEvent,
	) => void;
	selectedFiles: Set<string>;
	onNavigateToFolder: (path: string) => void;
	onDownload: (path: string, isDirectory: boolean) => void;
	onDelete: (path: string) => void;
	onRename: (path: string, currentName: string) => void;
	onCopy: (path: string) => void;
	onMove: (path: string) => void;
	renamingPath: string | null;
	onRenameConfirm: (newName: string) => void;
	onRenameCancel: () => void;
	onOpenInCanvas?: (path: string) => void;
	loadingDirs?: Set<string>;
}) {
	const isDir = node.type === "directory";
	const isExpanded = expanded[node.path];
	const isSelected = selectedFiles.has(node.path);
	const isRenaming = renamingPath === node.path;
	const isLoading = loadingDirs?.has(node.path) ?? false;

	// Sort children: directories first, then files
	const sortedChildren = node.children
		? [...node.children].sort((a, b) => {
				if (a.type === "directory" && b.type !== "directory") return -1;
				if (a.type !== "directory" && b.type === "directory") return 1;
				return a.name.localeCompare(b.name);
			})
		: [];

	const handleClick = (e: React.MouseEvent) => {
		e.stopPropagation();
		if (e.shiftKey) {
			// Shift+click: select/multi-select
			onSelectFile(node.path, node.name, isDir, e);
		} else if (isDir) {
			// Click on folder: expand/collapse
			onToggle(node.path);
		} else {
			// Click on file: preview
			onSelectFile(node.path, node.name, isDir, e);
		}
	};

	const handleDoubleClick = () => {
		if (isDir) {
			onNavigateToFolder(node.path);
		}
	};

	return (
		<li>
			<FileContextMenu
				node={node}
				onDownload={onDownload}
				onDelete={onDelete}
				onRename={onRename}
				onCopy={onCopy}
				onMove={onMove}
				onOpenInCanvas={onOpenInCanvas}
			>
				<button
					type="button"
					className={cn(
						"flex items-center gap-1.5 py-1.5 px-2 cursor-pointer transition-colors w-full",
						isSelected
							? "bg-primary/10 text-primary"
							: "hover:bg-muted text-muted-foreground hover:text-foreground",
					)}
					style={{ paddingLeft: `${level * 16 + 8}px` }}
					onClick={isRenaming ? undefined : handleClick}
					onDoubleClick={isRenaming ? undefined : handleDoubleClick}
				>
					{isDir ? (
						<span className="flex-shrink-0 text-muted-foreground">
							{isExpanded ? (
								<ChevronDown className="w-4 h-4" />
							) : (
								<ChevronRight className="w-4 h-4" />
							)}
						</span>
					) : (
						<span className="w-4 flex-shrink-0" />
					)}
					<FileIcon
						filename={node.name}
						isDirectory={isDir}
						size={18}
						className="flex-shrink-0"
					/>
					{isRenaming ? (
						<RenameInput
							initialValue={node.name}
							onConfirm={onRenameConfirm}
							onCancel={onRenameCancel}
						/>
					) : (
						<span className="truncate text-sm">{node.name}</span>
					)}
					{!isRenaming && isDir && node.children && (
						<span className="text-xs text-muted-foreground/60 ml-auto pr-2">
							{node.children.length}
						</span>
					)}
				</button>
			</FileContextMenu>
			{isDir &&
				isExpanded &&
				(sortedChildren.length > 0 ? (
					<ul>
						{sortedChildren.map((child) => (
							<TreeRow
								key={child.path}
								node={child}
								level={level + 1}
								expanded={expanded}
								onToggle={onToggle}
								onSelectFile={onSelectFile}
								selectedFiles={selectedFiles}
								onNavigateToFolder={onNavigateToFolder}
								onDownload={onDownload}
								onDelete={onDelete}
								onRename={onRename}
								onCopy={onCopy}
								onMove={onMove}
								renamingPath={renamingPath}
								onRenameConfirm={onRenameConfirm}
								onRenameCancel={onRenameCancel}
								onOpenInCanvas={onOpenInCanvas}
								loadingDirs={loadingDirs}
							/>
						))}
					</ul>
				) : isLoading ? (
					<div
						className="flex items-center gap-2 py-1.5 text-xs text-muted-foreground"
						style={{ paddingLeft: `${(level + 1) * 16 + 8}px` }}
					>
						<Loader2 className="w-3 h-3 animate-spin" />
						Loading...
					</div>
				) : null)}
		</li>
	);
}

// List View Component
function ListView({
	files,
	selectedFiles,
	onSelectFile,
	onNavigateToFolder,
	onDownload,
	onDelete,
	onRename,
	onCopy,
	onMove,
	renamingPath,
	onRenameConfirm,
	onRenameCancel,
	onOpenInCanvas,
}: {
	files: FileNode[];
	selectedFiles: Set<string>;
	onSelectFile: (
		path: string,
		name: string,
		isDirectory: boolean,
		event?: React.MouseEvent,
	) => void;
	onNavigateToFolder: (path: string) => void;
	onDownload: (path: string, isDirectory: boolean) => void;
	onDelete: (path: string) => void;
	onRename: (path: string, currentName: string) => void;
	onCopy: (path: string) => void;
	onMove: (path: string) => void;
	renamingPath: string | null;
	onRenameConfirm: (newName: string) => void;
	onRenameCancel: () => void;
	onOpenInCanvas?: (path: string) => void;
}) {
	// Sort: directories first, then files
	const sortedFiles = [...files].sort((a, b) => {
		if (a.type === "directory" && b.type !== "directory") return -1;
		if (a.type !== "directory" && b.type === "directory") return 1;
		return a.name.localeCompare(b.name);
	});

	return (
		<div className="min-w-full">
			{/* Header */}
			<div className="sticky top-0 bg-card z-10 flex items-center gap-2 px-3 py-2 border-b border-border text-xs text-muted-foreground font-medium">
				<div className="flex-1 min-w-0">Name</div>
				<div className="w-24 text-right hidden sm:block">Modified</div>
				<div className="w-20 text-right hidden sm:block">Size</div>
			</div>

			{/* Files */}
			<div className="divide-y divide-border/50">
				{sortedFiles.map((file) => {
					const isSelected = selectedFiles.has(file.path);
					const isRenaming = renamingPath === file.path;
					return (
						<FileContextMenu
							key={file.path}
							node={file}
							onDownload={onDownload}
							onOpenInCanvas={onOpenInCanvas}
							onDelete={onDelete}
							onRename={onRename}
							onCopy={onCopy}
							onMove={onMove}
						>
							<button
								type="button"
								onClick={(e) => {
									if (isRenaming) return;
									const isDir = file.type === "directory";
									if (e.shiftKey) {
										// Shift+click: select/multi-select
										onSelectFile(file.path, file.name, isDir, e);
									} else if (isDir) {
										// Click on folder: navigate into it
										onNavigateToFolder(file.path);
									} else {
										// Click on file: preview
										onSelectFile(file.path, file.name, isDir, e);
									}
								}}
								onDoubleClick={() => {
									// Double-click does nothing special now (single click navigates folders)
								}}
								className={cn(
									"flex items-center gap-2 px-3 py-2 transition-colors cursor-pointer w-full",
									isSelected ? "bg-primary/10" : "hover:bg-muted/50",
								)}
							>
								<div className="flex-1 min-w-0 flex items-center gap-2">
									<FileIcon
										filename={file.name}
										isDirectory={file.type === "directory"}
										size={20}
									/>
									{isRenaming ? (
										<RenameInput
											initialValue={file.name}
											onConfirm={onRenameConfirm}
											onCancel={onRenameCancel}
										/>
									) : (
										<span className="truncate text-sm">{file.name}</span>
									)}
									{!isRenaming &&
										file.type === "directory" &&
										file.children && (
											<span className="text-xs text-muted-foreground/60">
												({file.children.length})
											</span>
										)}
								</div>
								<div className="w-24 text-right text-xs text-muted-foreground hidden sm:block">
									{formatDate(file.modified)}
								</div>
								<div className="w-20 text-right text-xs text-muted-foreground hidden sm:block">
									{file.type === "file" ? formatFileSize(file.size) : "-"}
								</div>
							</button>
						</FileContextMenu>
					);
				})}
			</div>
		</div>
	);
}

// Grid View Component
function GridView({
	files,
	selectedFiles,
	onSelectFile,
	onNavigateToFolder,
	onDownload,
	onDelete,
	onRename,
	onCopy,
	onMove,
	renamingPath,
	onRenameConfirm,
	onRenameCancel,
	onOpenInCanvas,
}: {
	files: FileNode[];
	selectedFiles: Set<string>;
	onSelectFile: (
		path: string,
		name: string,
		isDirectory: boolean,
		event?: React.MouseEvent,
	) => void;
	onNavigateToFolder: (path: string) => void;
	onDownload: (path: string, isDirectory: boolean) => void;
	onDelete: (path: string) => void;
	onRename: (path: string, currentName: string) => void;
	onCopy: (path: string) => void;
	onMove: (path: string) => void;
	renamingPath: string | null;
	onRenameConfirm: (newName: string) => void;
	onRenameCancel: () => void;
	onOpenInCanvas?: (path: string) => void;
}) {
	// Sort: directories first, then files
	const sortedFiles = [...files].sort((a, b) => {
		if (a.type === "directory" && b.type !== "directory") return -1;
		if (a.type !== "directory" && b.type === "directory") return 1;
		return a.name.localeCompare(b.name);
	});

	return (
		<div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2 p-2">
			{sortedFiles.map((file) => {
				const isSelected = selectedFiles.has(file.path);
				const isRenaming = renamingPath === file.path;
				return (
					<FileContextMenu
						key={file.path}
						node={file}
						onDownload={onDownload}
						onDelete={onDelete}
						onRename={onRename}
						onCopy={onCopy}
						onMove={onMove}
						onOpenInCanvas={onOpenInCanvas}
					>
						<button
							type="button"
							onClick={(e) => {
								if (isRenaming) return;
								const isDir = file.type === "directory";
								if (e.shiftKey) {
									// Shift+click: select/multi-select
									onSelectFile(file.path, file.name, isDir, e);
								} else if (isDir) {
									// Click on folder: navigate into it
									onNavigateToFolder(file.path);
								} else {
									// Click on file: preview
									onSelectFile(file.path, file.name, isDir, e);
								}
							}}
							onDoubleClick={() => {
								// Double-click does nothing special now
							}}
							className={cn(
								"flex flex-col items-center gap-2 p-3 rounded-lg cursor-pointer transition-colors hover:bg-muted/50",
								isSelected && "bg-primary/10 ring-1 ring-primary/30",
							)}
						>
							<FileIcon
								filename={file.name}
								isDirectory={file.type === "directory"}
								size={48}
							/>
							{isRenaming ? (
								<RenameInput
									initialValue={file.name}
									onConfirm={onRenameConfirm}
									onCancel={onRenameCancel}
								/>
							) : (
								<span
									className="text-xs text-center truncate w-full"
									title={file.name}
								>
									{file.name}
								</span>
							)}
						</button>
					</FileContextMenu>
				);
			})}
		</div>
	);
}
