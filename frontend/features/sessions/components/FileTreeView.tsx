"use client";

import { FileIcon } from "@/components/data-display";
import { ThumbnailImage } from "./ThumbnailImage";
import { MediaQuickAccessBar, type MediaType } from "./MediaQuickAccessBar";
import { LightboxGallery, type LightboxItem } from "./LightboxGallery";
import { getThumbnailUrl, supportsThumbnail, supportsMediaThumbnail, isVideoFile, formatDuration } from "@/lib/thumbnail-utils";
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
import { listWorkspaceSessions } from "@/lib/api/sessions";
import {
	copyPathMux,
	copyToWorkspaceMux,
	createDirectoryMux,
	deletePathMux,
	downloadPathMux,
	downloadZipMux,
	fetchFileTreeMux,
	movePathMux,
	renamePathMux,
	unwatchFilesMux,
	uploadFileMux,
	watchFilesMux,
} from "@/lib/mux-files";
import { normalizeWorkspacePath } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import { getWsManager } from "@/lib/ws-manager";
import {
	AppWindow,
	ChevronDown,
	ChevronRight,
	Copy,
	Download,
	Folder,
	FolderPlus,
	FolderSync,
	FolderUp,
	Home,
	LayoutGrid,
	List,
	Loader2,
	Maximize2,
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

/**
 * Clear all tree cache entries for a given workspace.
 * Call this after any mutation operation to ensure fresh data on next fetch.
 */
function clearTreeCache(workspaceKey?: string): void {
	if (workspaceKey) {
		// Clear only cache entries for the specific workspace
		const prefix = `${workspaceKey}:`;
		for (const key of treeCache.keys()) {
			if (key.startsWith(prefix)) {
				treeCache.delete(key);
			}
		}
	} else {
		// Clear all cache
		treeCache.clear();
	}
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

	const request = (async () => {
		try {
			// Fast path for normal cases.
			return await fetchFileTreeMux(workspacePath, path, depth, false, 12000);
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			const isTimeout = message.includes("Request timeout: tree");
			if (!isTimeout) {
				// Connection-loss and protocol errors should surface immediately.
				// Retrying with a long timeout here causes the UI to look stuck.
				throw error;
			}
			// Retry once with a longer timeout for large/shared workspaces.
			console.warn("[file-tree] initial fetch timed out, retrying", {
				workspacePath,
				path,
				depth,
				error: message,
			});
			return fetchFileTreeMux(workspacePath, path, depth, false, 30000);
		}
	})().finally(() => {
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
	mediaFilter: MediaType;
}

export const initialFileTreeState: FileTreeState = {
	currentPath: ".",
	expanded: {},
	selectedFile: null,
	selectedFiles: new Set(),
	viewMode: "tree",
	mediaFilter: "all",
};

interface FileTreeViewProps {
	onPreviewFile?: (filePath: string) => void;
	onOpenInCanvas?: (filePath: string) => void;
	onOpenAsApp?: (filePath: string) => void;
	workspacePath?: string | null;
	/** External state for persistence across view switches */
	state?: FileTreeState;
	/** Callback to update external state */
	onStateChange?: (state: FileTreeState) => void;
}

export function FileTreeView({
	onPreviewFile,
	onOpenInCanvas,
	onOpenAsApp,
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
	const [retryAttempt, setRetryAttempt] = useState(0);
	const [isRecovering, setIsRecovering] = useState(false);
	const [newFolderName, setNewFolderName] = useState<string | null>(null);
	const [renamingPath, setRenamingPath] = useState<string | null>(null);
	const [loadingDirs, setLoadingDirs] = useState<Set<string>>(new Set());
	const [pathDialog, setPathDialog] = useState<{
		title: string;
		sourcePath: string;
		onConfirm: (value: string) => void;
	} | null>(null);
	const [workspacePickerState, setWorkspacePickerState] = useState<{
		sourcePath: string;
		sourceName: string;
		isDirectory: boolean;
	} | null>(null);
	const fileInputRef = useRef<HTMLInputElement>(null);
	const lastLoadRef = useRef<{ key: string; ts: number } | null>(null);

	// Media gallery state
	const [lightboxOpen, setLightboxOpen] = useState(false);
	const [lightboxIndex, setLightboxIndex] = useState(0);
	const [mediaSearchQuery, setMediaSearchQuery] = useState("");

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
	const [internalMediaFilter, setInternalMediaFilter] = useState<MediaType>("all");
	const [internalCurrentPath, setInternalCurrentPath] = useState<string>(".");

	const expanded = state?.expanded ?? internalExpanded;
	const selectedFile = state?.selectedFile ?? internalSelectedFile;
	const selectedFiles = state?.selectedFiles ?? internalSelectedFiles;
	const viewMode = state?.viewMode ?? internalViewMode;
	const currentPath = state?.currentPath ?? internalCurrentPath;
	const mediaFilter = state?.mediaFilter ?? internalMediaFilter;

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
				if (updates.mediaFilter !== undefined)
					setInternalMediaFilter(updates.mediaFilter);
			}
		},
		[onStateChange],
	);

	const cacheKey = normalizedWorkspacePath ?? null;
	const isTransientTreeError = useCallback((message: string) => {
		return (
			message.includes("Connection lost") ||
			message.includes("Request timeout: tree") ||
			message.includes("WebSocket")
		);
	}, []);

	const loadTree = useCallback(
		async (
			path: string,
			preserveState = false,
			skipCache = false,
			silent = false,
		) => {
			if (!normalizedWorkspacePath || !cacheKey) return;
			const requestKey = getTreeCacheKey(cacheKey, path, INITIAL_DEPTH);
			const now = Date.now();
			if (
				!skipCache &&
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
				setRetryAttempt(0);
				setIsRecovering(false);
				if (!preserveState) {
					updateState({ currentPath: path });
				}
				if (!silent) setError("");
			} catch (err) {
				const message =
					err instanceof Error ? err.message : "Unable to load file tree";
				if (!silent) {
					setError(message);
				}
				if (isTransientTreeError(message)) {
					setIsRecovering(true);
					setRetryAttempt((prev) => prev + 1);
				}
			} finally {
				if (!silent) setLoading(false);
			}
		},
		[normalizedWorkspacePath, updateState, cacheKey, isTransientTreeError],
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
		if (!normalizedWorkspacePath) {
			setLoading(false);
			setIsRecovering(false);
			setRetryAttempt(0);
			return;
		}
		loadTreeRef.current(currentPath, true);
	}, [currentPath, normalizedWorkspacePath]);

	// Never-stuck recovery: when we are in transient error state, keep retrying
	// with bounded backoff and trigger immediate retry on mux reconnect.
	useEffect(() => {
		if (!normalizedWorkspacePath || !isRecovering) return;

		const manager = getWsManager();
		const delay = Math.min(8000, 500 * 2 ** Math.min(retryAttempt, 4));
		const timer = setTimeout(() => {
			void loadTreeRef.current(currentPath, true, true);
		}, delay);

		const unsubscribeState = manager.onConnectionState((state) => {
			if (state === "connected") {
				void loadTreeRef.current(currentPath, true, true);
			}
		});

		return () => {
			clearTimeout(timer);
			unsubscribeState();
		};
	}, [normalizedWorkspacePath, isRecovering, retryAttempt, currentPath]);

	// Watch workspace for file changes via inotify (backend-side).
	// When files are created/modified/deleted, the backend pushes events
	// through the WebSocket. We debounce and refresh the tree.
	useEffect(() => {
		if (!normalizedWorkspacePath) return;

		// Start watching this workspace on the server
		watchFilesMux(normalizedWorkspacePath).catch(() => {});

		let debounceTimer: ReturnType<typeof setTimeout> | null = null;
		let unsub: (() => void) | undefined;

		try {
			const ws = getWsManager();
			// biome-ignore lint: event type is opaque from subscribe
			unsub = ws.subscribe("files", (event: any) => {
				if (
					event.type !== "file_changed" ||
					event.workspace_path !== normalizedWorkspacePath
				) {
					return;
				}
				// Debounce: wait 500ms after last event before refreshing
				if (debounceTimer) clearTimeout(debounceTimer);
				debounceTimer = setTimeout(() => {
					loadTreeRef.current(currentPath, true, true, true);
				}, 500);
			});
		} catch {
			// WS manager not yet initialized
		}

		return () => {
			unsub?.();
			if (debounceTimer) clearTimeout(debounceTimer);
			unwatchFilesMux(normalizedWorkspacePath).catch(() => {});
		};
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
			// Normal click: clear selection
			updateState({ selectedFile: path, selectedFiles: new Set() });

			if (isDirectory) {
				// Click on folder: navigate into it
				handleNavigateToFolder(path);
			} else if (!isDirectory && onPreviewFile && isPreviewable(name)) {
				// Other previewable file: use existing preview
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
			// Clear cache and refresh to show new files immediately
			clearTreeCache(cacheKey);
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

	const handleDownload = (path: string, _isDirectory: boolean) => {
		if (!normalizedWorkspacePath || !cacheKey) return;
		void downloadPathMux(normalizedWorkspacePath, path);
	};

	const handleDownloadSelected = () => {
		if (!normalizedWorkspacePath || !cacheKey || selectedFiles.size === 0)
			return;

		void (async () => {
			try {
				const selectedPaths = Array.from(selectedFiles);
				if (selectedPaths.length === 1) {
					await downloadPathMux(normalizedWorkspacePath, selectedPaths[0]);
					return;
				}

				const zipName = `selection-${new Date().toISOString().slice(0, 10)}.zip`;
				await downloadZipMux(normalizedWorkspacePath, selectedPaths, zipName);
			} catch (err) {
				setError(err instanceof Error ? err.message : "Download failed");
			}
		})();
	};

	const handleDelete = async (path: string) => {
		if (!normalizedWorkspacePath || !cacheKey) return;

		try {
			await deletePathMux(normalizedWorkspacePath, path, true);
			// Clear cache and refresh to show changes immediately
			clearTreeCache(cacheKey);
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
			// Clear cache and refresh to show changes immediately
			clearTreeCache(cacheKey);
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
					// Clear cache and refresh to show changes immediately
					clearTreeCache(cacheKey);
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
					// Clear cache and refresh to show changes immediately
					clearTreeCache(cacheKey);
					await refreshTree();
				} catch (err) {
					setError(err instanceof Error ? err.message : "Move failed");
				}
			},
		});
	};

	const handleCopyToWorkspace = (
		path: string,
		name: string,
		isDirectory: boolean,
	) => {
		setWorkspacePickerState({
			sourcePath: path,
			sourceName: name,
			isDirectory,
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
				// Clear cache and refresh to show new folder immediately
				clearTreeCache(cacheKey);
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
				// Clear cache and refresh to show renamed file immediately
				clearTreeCache(cacheKey);
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

	// Count media files in tree
	const countMediaFiles = () => {
		let imageCount = 0;
		let videoCount = 0;
		let audioCount = 0;

		const countFiles = (nodes: FileNode[]) => {
			for (const node of nodes) {
				if (node.type === "directory") {
					if (node.children) {
						countFiles(node.children);
					}
				} else {
					const ext = node.name.substring(node.name.lastIndexOf(".")).toLowerCase();
					if (node.name === ".") continue;

					if (isVideoFile(node.name)) {
						videoCount++;
					} else if (supportsThumbnail(node.name)) {
						imageCount++;
					} else if ([".mp3", ".wav", ".flac", ".aac", ".m4a", ".opus"].includes(ext)) {
						audioCount++;
					}
				}
			}
		};

		countFiles(tree);
		return { imageCount, videoCount, audioCount };
	};

	// Filter files by media type
	const filterFiles = (nodes: FileNode[], filter: MediaType): FileNode[] => {
		if (filter === "all") return nodes;

		return nodes.filter((node) => {
			if (node.type === "directory") {
				// Always show directories, but filter their children
				if (node.children) {
					const filteredChildren = filterFiles(node.children, filter);
					return filteredChildren.length > 0 || node.children.length === 0;
				}
				return true;
			}

			// Filter files by type
			const ext = node.name.substring(node.name.lastIndexOf(".")).toLowerCase();
			if (filter === "images") {
				return supportsThumbnail(node.name);
			} else if (filter === "videos") {
				return isVideoFile(node.name);
			} else if (filter === "audio") {
				return [".mp3", ".wav", ".flac", ".aac", ".m4a", ".opus"].includes(ext);
			}
			return true;
		}).map((node) => {
			// Recursively filter children for directories
			if (node.type === "directory" && node.children) {
				return {
					...node,
					children: filterFiles(node.children, filter),
				};
			}
			return node;
		});
	};

	// Get filtered tree based on media filter and search query
	const filteredTree = useMemo(() => {
		let result = filterFiles(tree, viewMode === "tree" ? "all" : mediaFilter);

		// Apply search filter
		const query = mediaSearchQuery.trim().toLowerCase();
		if (query && viewMode !== "tree") {
			const searchFilter = (nodes: FileNode[]): FileNode[] => {
				return nodes.filter((node) => {
					if (node.type === "directory") {
						if (node.children) {
							const filtered = searchFilter(node.children);
							return filtered.length > 0;
						}
						return node.name.toLowerCase().includes(query);
					}
					return node.name.toLowerCase().includes(query);
				}).map((node) => {
					if (node.type === "directory" && node.children) {
						return { ...node, children: searchFilter(node.children) };
					}
					return node;
				});
			};
			result = searchFilter(result);
		}

		return result;
	}, [tree, mediaFilter, viewMode, mediaSearchQuery]);

	const mediaCounts = useMemo(() => countMediaFiles(), [tree]);

	// Build lightbox items from media files in current tree
	const lightboxItems: LightboxItem[] = useMemo(() => {
		const items: LightboxItem[] = [];
		const collectMedia = (nodes: FileNode[]) => {
			for (const node of nodes) {
				if (node.type === "directory") {
					if (node.children) collectMedia(node.children);
				} else if (normalizedWorkspacePath) {
					const isImg = supportsThumbnail(node.name);
					const isVid = isVideoFile(node.name);
					if (isImg || isVid) {
						const params = new URLSearchParams({
							directory: normalizedWorkspacePath,
							path: node.path,
						});
						items.push({
							src: `/api/files/file?${params.toString()}`,
							type: isVid ? "video" : "image",
							path: node.path,
							filename: node.name,
						});
					}
				}
			}
		};
		collectMedia(filteredTree);
		return items;
	}, [filteredTree, normalizedWorkspacePath]);

	// Open lightbox for a specific file
	const handleOpenLightbox = useCallback((filePath: string) => {
		const idx = lightboxItems.findIndex((item) => item.path === filePath);
		if (idx >= 0) {
			setLightboxIndex(idx);
			setLightboxOpen(true);
		}
	}, [lightboxItems]);

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
		return (
			<div className="p-4 text-sm text-destructive">
				{error}
				{isRecovering && (
					<span className="ml-2 text-muted-foreground">
						Retrying automatically (attempt {retryAttempt})...
					</span>
				)}
			</div>
		);
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

			{/* Media Quick Access Bar */}
			{viewMode !== "tree" && (
				<MediaQuickAccessBar
					activeFilter={mediaFilter}
					onFilterChange={(filter) => updateState({ mediaFilter: filter })}
					imageCount={mediaCounts.imageCount}
					videoCount={mediaCounts.videoCount}
					audioCount={mediaCounts.audioCount}
					searchQuery={mediaSearchQuery}
					onSearchChange={setMediaSearchQuery}
				/>
			)}

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
				<div className="flex-shrink-0 flex items-center gap-2 px-3 py-1.5 bg-primary/10 border-b border-border text-xs">
					<span className="text-primary font-medium">
						{selectedFiles.size} item(s) selected
					</span>
					<span className="text-muted-foreground">
						{(() => {
							let totalSize = 0;
							const findSize = (nodes: FileNode[]) => {
								for (const node of nodes) {
									if (selectedFiles.has(node.path) && node.size) {
										totalSize += node.size;
									}
									if (node.children) findSize(node.children);
								}
							};
							findSize(tree);
							return totalSize > 0 ? formatFileSize(totalSize) : "";
						})()}
					</span>
					<div className="flex-1" />
					<button
						type="button"
						onClick={handleDownloadSelected}
						className="px-2 py-0.5 bg-primary text-primary-foreground rounded hover:bg-primary/90 transition-colors"
						title="Download selected"
					>
						Download
					</button>
					<button
						type="button"
						onClick={() => {
							// Open lightbox with only selected media files
							const mediaFiles = Array.from(selectedFiles).filter((p) => {
								const name = p.split("/").pop() ?? "";
								return supportsThumbnail(name) || isVideoFile(name);
							});
							if (mediaFiles.length > 0 && normalizedWorkspacePath) {
								const items: LightboxItem[] = mediaFiles.map((p) => {
									const name = p.split("/").pop() ?? "";
									const params = new URLSearchParams({
										directory: normalizedWorkspacePath,
										path: p,
									});
									return {
										src: `/api/files/file?${params.toString()}`,
										type: isVideoFile(name) ? "video" as const : "image" as const,
										path: p,
										filename: name,
									};
								});
								setLightboxIndex(0);
								setLightboxOpen(true);
							}
						}}
						className="px-2 py-0.5 bg-muted text-foreground rounded hover:bg-muted/80 transition-colors"
						title="View selected in gallery"
					>
						Gallery
					</button>
					<button
						type="button"
						onClick={handleDeleteSelected}
						className="px-2 py-0.5 bg-destructive/10 text-destructive rounded hover:bg-destructive/20 transition-colors"
						title="Delete selected"
					>
						Delete
					</button>
					<button
						type="button"
						onClick={clearSelection}
						className="text-muted-foreground hover:text-foreground ml-1"
					>
						Clear
					</button>
				</div>
			)}

			{/* Error message */}
			{error && (
				<div className="flex-shrink-0 px-3 py-2 bg-destructive/10 text-destructive text-xs">
					{error}
					{isRecovering && (
						<span className="ml-2 text-muted-foreground">
							Retrying automatically (attempt {retryAttempt})...
						</span>
					)}
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
				{filteredTree.length === 0 ? (
					<div className="text-sm text-muted-foreground p-4">
						No files found.
					</div>
				) : viewMode === "tree" ? (
					<TreeView
						nodes={filteredTree}
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
						onCopyToWorkspace={handleCopyToWorkspace}
						renamingPath={renamingPath}
						onRenameConfirm={handleConfirmRename}
						onRenameCancel={handleCancelRename}
						onOpenInCanvas={onOpenInCanvas}
						onOpenAsApp={onOpenAsApp}
						onOpenInGallery={handleOpenLightbox}
						loadingDirs={loadingDirs}
					/>
				) : viewMode === "list" ? (
					<ListView
						files={filteredTree}
						selectedFiles={selectedFiles}
						onSelectFile={handleSelectFile}
						onNavigateToFolder={handleNavigateToFolder}
						onDownload={handleDownload}
						onDelete={handleDelete}
						onRename={handleStartRename}
						onCopy={handleCopy}
						onMove={handleMove}
						onCopyToWorkspace={handleCopyToWorkspace}
						renamingPath={renamingPath}
						onRenameConfirm={handleConfirmRename}
						onRenameCancel={handleCancelRename}
						onOpenInCanvas={onOpenInCanvas}
						onOpenAsApp={onOpenAsApp}
						onOpenInGallery={handleOpenLightbox}
					/>
				) : (
					<GridView
						files={filteredTree}
						workspacePath={normalizedWorkspacePath}
						selectedFiles={selectedFiles}
						onSelectFile={handleSelectFile}
						onNavigateToFolder={handleNavigateToFolder}
						onDownload={handleDownload}
						onDelete={handleDelete}
						onRename={handleStartRename}
						onCopy={handleCopy}
						onMove={handleMove}
						onCopyToWorkspace={handleCopyToWorkspace}
						renamingPath={renamingPath}
						onRenameConfirm={handleConfirmRename}
						onRenameCancel={handleCancelRename}
						onOpenInCanvas={onOpenInCanvas}
						onOpenAsApp={onOpenAsApp}
						onOpenInGallery={handleOpenLightbox}
					/>
				)}
			</div>

			{/* Destination picker dialog for copy/move operations */}
			<DestinationPickerDialog
				open={pathDialog !== null}
				title={pathDialog?.title ?? ""}
				sourcePath={pathDialog?.sourcePath ?? ""}
				tree={filteredTree}
				onConfirm={(value) => {
					pathDialog?.onConfirm(value);
					setPathDialog(null);
				}}
				onCancel={() => setPathDialog(null)}
			/>

			{/* Workspace picker dialog for cross-workspace copy */}
			<WorkspacePickerDialog
				open={workspacePickerState !== null}
				sourceName={workspacePickerState?.sourceName ?? ""}
				sourcePath={workspacePickerState?.sourcePath ?? ""}
				isDirectory={workspacePickerState?.isDirectory ?? false}
				currentWorkspacePath={normalizedWorkspacePath ?? ""}
				onConfirm={async (targetWorkspace, targetPath) => {
					if (!normalizedWorkspacePath) return;
					try {
						const count = await copyToWorkspaceMux(
							normalizedWorkspacePath,
							workspacePickerState?.sourcePath ?? "",
							targetWorkspace,
							targetPath,
						);
						setWorkspacePickerState(null);
						setError("");
						// Brief success indicator (clear after 3s)
						setError(
							`Copied ${count} file${count !== 1 ? "s" : ""} to workspace`,
						);
						setTimeout(() => setError(""), 3000);
					} catch (err) {
						setError(
							err instanceof Error
								? err.message
								: "Cross-workspace copy failed",
						);
					}
				}}
				onCancel={() => setWorkspacePickerState(null)}
			/>

			{/* Lightbox Gallery */}
			<LightboxGallery
				open={lightboxOpen}
				items={lightboxItems}
				initialIndex={lightboxIndex}
				onClose={() => setLightboxOpen(false)}
				workspacePath={normalizedWorkspacePath}
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
	onOpenAsApp,
	onOpenInGallery,
	onCopyToWorkspace,
}: {
	children: React.ReactNode;
	node: FileNode;
	onDownload: (path: string, isDirectory: boolean) => void;
	onDelete: (path: string) => void;
	onRename: (path: string, currentName: string) => void;
	onCopy: (path: string) => void;
	onMove: (path: string) => void;
	onOpenInCanvas?: (path: string) => void;
	onOpenAsApp?: (path: string) => void;
	onOpenInGallery?: (path: string) => void;
	onOpenInGallery?: (path: string) => void;
	onCopyToWorkspace?: (
		path: string,
		name: string,
		isDirectory: boolean,
	) => void;
}) {
	const isImage = node.type === "file" && isImageFile(node.name);
	const isVideo = node.type === "file" && isVideoFile(node.name);
	const isMedia = isImage || isVideo;
	const isHtml = node.type === "file" && /\.html?$/i.test(node.name);

	return (
		<ContextMenu>
			<ContextMenuTrigger className="contents">{children}</ContextMenuTrigger>
			<ContextMenuContent>
				{isMedia && onOpenInGallery && (
					<>
						<ContextMenuItem onClick={() => onOpenInGallery(node.path)}>
							<Maximize2 className="w-4 h-4 mr-2" />
							Open in Gallery
						</ContextMenuItem>
						<ContextMenuSeparator />
					</>
				)}
				{isHtml && onOpenAsApp && (
					<>
						<ContextMenuItem onClick={() => onOpenAsApp(node.path)}>
							<AppWindow className="w-4 h-4 mr-2" />
							Open as App
						</ContextMenuItem>
						<ContextMenuSeparator />
					</>
				)}
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
				{onCopyToWorkspace && (
					<ContextMenuItem
						onClick={() =>
							onCopyToWorkspace(node.path, node.name, node.type === "directory")
						}
					>
						<FolderSync className="w-4 h-4 mr-2" />
						Copy to Workspace
					</ContextMenuItem>
				)}
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
	onCopyToWorkspace,
	renamingPath,
	onRenameConfirm,
	onRenameCancel,
	onOpenInCanvas,
	onOpenAsApp,
	onOpenInGallery,
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
	onCopyToWorkspace?: (
		path: string,
		name: string,
		isDirectory: boolean,
	) => void;
	renamingPath: string | null;
	onRenameConfirm: (newName: string) => void;
	onRenameCancel: () => void;
	onOpenInCanvas?: (path: string) => void;
	onOpenAsApp?: (path: string) => void;
	onOpenInGallery?: (path: string) => void;
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
					onCopyToWorkspace={onCopyToWorkspace}
					renamingPath={renamingPath}
					onRenameConfirm={onRenameConfirm}
					onRenameCancel={onRenameCancel}
					onOpenInCanvas={onOpenInCanvas}
					onOpenAsApp={onOpenAsApp}
					onOpenInGallery={onOpenInGallery}
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
	onCopyToWorkspace,
	renamingPath,
	onRenameConfirm,
	onRenameCancel,
	onOpenInCanvas,
	onOpenAsApp,
	onOpenInGallery,
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
	onCopyToWorkspace?: (
		path: string,
		name: string,
		isDirectory: boolean,
	) => void;
	renamingPath: string | null;
	onRenameConfirm: (newName: string) => void;
	onRenameCancel: () => void;
	onOpenInCanvas?: (path: string) => void;
	onOpenAsApp?: (path: string) => void;
	onOpenInGallery?: (path: string) => void;
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
				onCopyToWorkspace={onCopyToWorkspace}
				onOpenInCanvas={onOpenInCanvas}
				onOpenAsApp={onOpenAsApp}
				onOpenInGallery={onOpenInGallery}
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
						<span className="truncate text-sm" title={node.name}>
							{node.name}
						</span>
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
								onCopyToWorkspace={onCopyToWorkspace}
								renamingPath={renamingPath}
								onRenameConfirm={onRenameConfirm}
								onRenameCancel={onRenameCancel}
								onOpenInCanvas={onOpenInCanvas}
								onOpenAsApp={onOpenAsApp}
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
	onCopyToWorkspace,
	renamingPath,
	onRenameConfirm,
	onRenameCancel,
	onOpenInCanvas,
	onOpenAsApp,
	onOpenInGallery,
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
	onCopyToWorkspace?: (
		path: string,
		name: string,
		isDirectory: boolean,
	) => void;
	renamingPath: string | null;
	onRenameConfirm: (newName: string) => void;
	onRenameCancel: () => void;
	onOpenInCanvas?: (path: string) => void;
	onOpenAsApp?: (path: string) => void;
	onOpenInGallery?: (path: string) => void;
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
							onOpenAsApp={onOpenAsApp}
							onDelete={onDelete}
							onRename={onRename}
							onCopy={onCopy}
							onMove={onMove}
							onCopyToWorkspace={onCopyToWorkspace}
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
										className="flex-shrink-0"
									/>
									{isRenaming ? (
										<RenameInput
											initialValue={file.name}
											onConfirm={onRenameConfirm}
											onCancel={onRenameCancel}
										/>
									) : (
										<span className="truncate text-sm" title={file.name}>
											{file.name}
										</span>
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
	workspacePath,
	selectedFiles,
	onSelectFile,
	onNavigateToFolder,
	onDownload,
	onDelete,
	onRename,
	onCopy,
	onMove,
	onCopyToWorkspace,
	renamingPath,
	onRenameConfirm,
	onRenameCancel,
	onOpenInCanvas,
	onOpenAsApp,
	onOpenInGallery,
}: {
	files: FileNode[];
	workspacePath?: string | null;
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
	onCopyToWorkspace?: (
		path: string,
		name: string,
		isDirectory: boolean,
	) => void;
	renamingPath: string | null;
	onRenameConfirm: (newName: string) => void;
	onRenameCancel: () => void;
	onOpenInCanvas?: (path: string) => void;
	onOpenAsApp?: (path: string) => void;
	onOpenInGallery?: (path: string) => void;
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
						onCopyToWorkspace={onCopyToWorkspace}
						onOpenInCanvas={onOpenInCanvas}
						onOpenAsApp={onOpenAsApp}
						onOpenInGallery={onOpenInGallery}
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
							{file.type === "file" && supportsMediaThumbnail(file.name) && workspacePath ? (
								<ThumbnailImage
									src={getThumbnailUrl({ workspacePath, filePath: file.path })}
									alt={file.name}
									filename={file.name}
									extension={file.name.substring(file.name.lastIndexOf("."))}
									isVideo={isVideoFile(file.name)}
									videoSrc={isVideoFile(file.name) ? `/api/files/file?${new URLSearchParams({ directory: workspacePath, path: file.path }).toString()}` : undefined}
									size={96}
									onClick={() => {
										if (onOpenInGallery) onOpenInGallery(file.path);
									}}
								/>
							) : (
								<FileIcon
									filename={file.name}
									isDirectory={file.type === "directory"}
									size={48}
								/>
							)}
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

// Workspace picker dialog for cross-workspace copy
const WorkspacePickerDialog = memo(function WorkspacePickerDialog({
	open,
	sourceName,
	sourcePath,
	isDirectory,
	currentWorkspacePath,
	onConfirm,
	onCancel,
}: {
	open: boolean;
	sourceName: string;
	sourcePath: string;
	isDirectory: boolean;
	currentWorkspacePath: string;
	onConfirm: (targetWorkspace: string, targetPath: string) => void;
	onCancel: () => void;
}) {
	const [workspaces, setWorkspaces] = useState<
		{ workspace_path: string; label: string }[]
	>([]);
	const [selectedWorkspace, setSelectedWorkspace] = useState<string | null>(
		null,
	);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState("");
	const [copying, setCopying] = useState(false);

	useEffect(() => {
		if (!open) return;

		setSelectedWorkspace(null);
		setError("");
		setCopying(false);
		setLoading(true);

		listWorkspaceSessions()
			.then((sessions) => {
				// Deduplicate by workspace_path and exclude the current workspace
				const seen = new Set<string>();
				const unique: { workspace_path: string; label: string }[] = [];
				for (const session of sessions) {
					if (
						session.workspace_path &&
						session.workspace_path !== currentWorkspacePath &&
						!seen.has(session.workspace_path)
					) {
						seen.add(session.workspace_path);
						// Use the last path segment as label, with full path as subtitle
						const parts = session.workspace_path.split("/");
						const dirName = parts[parts.length - 1] || session.workspace_path;
						unique.push({
							workspace_path: session.workspace_path,
							label: dirName,
						});
					}
				}
				// Sort alphabetically by label
				unique.sort((a, b) => a.label.localeCompare(b.label));
				setWorkspaces(unique);
			})
			.catch((err) => {
				setError(
					err instanceof Error ? err.message : "Failed to load workspaces",
				);
			})
			.finally(() => setLoading(false));
	}, [open, currentWorkspacePath]);

	const handleConfirm = async () => {
		if (!selectedWorkspace) return;
		setCopying(true);
		setError("");
		try {
			// Target path: place the file/directory at the root of the target workspace
			await onConfirm(selectedWorkspace, sourceName);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Copy failed");
		} finally {
			setCopying(false);
		}
	};

	return (
		<Dialog open={open} onOpenChange={(o) => !o && onCancel()}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>Copy to Workspace</DialogTitle>
					<DialogDescription>
						Copy{" "}
						<span className="font-medium text-foreground">{sourceName}</span>
						{isDirectory ? " (directory)" : ""} to another workspace.
					</DialogDescription>
				</DialogHeader>

				{loading ? (
					<div className="flex items-center justify-center py-8">
						<Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
					</div>
				) : workspaces.length === 0 ? (
					<div className="py-6 text-center text-sm text-muted-foreground">
						No other workspaces available. Create another session first.
					</div>
				) : (
					<div className="max-h-64 overflow-y-auto rounded-md border border-border bg-muted/30 py-1">
						{workspaces.map((ws) => (
							<button
								key={ws.workspace_path}
								type="button"
								onClick={() => setSelectedWorkspace(ws.workspace_path)}
								className={cn(
									"flex flex-col gap-0.5 w-full px-3 py-2 text-left rounded transition-colors",
									selectedWorkspace === ws.workspace_path
										? "bg-primary/15 text-primary"
										: "hover:bg-muted text-foreground",
								)}
							>
								<div className="flex items-center gap-2">
									<Folder className="w-4 h-4 shrink-0 text-muted-foreground" />
									<span className="text-sm font-medium truncate">
										{ws.label}
									</span>
								</div>
								<span className="text-xs text-muted-foreground truncate pl-6">
									{ws.workspace_path}
								</span>
							</button>
						))}
					</div>
				)}

				{error && <div className="text-sm text-destructive">{error}</div>}

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
						disabled={!selectedWorkspace || copying}
						className="inline-flex items-center justify-center rounded-md px-4 py-2 text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:pointer-events-none"
					>
						{copying ? (
							<>
								<Loader2 className="w-4 h-4 mr-2 animate-spin" />
								Copying...
							</>
						) : (
							"Copy"
						)}
					</button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
});
