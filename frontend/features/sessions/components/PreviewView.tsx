"use client";

import { Button } from "@/components/ui/button";
import { MarkdownRenderer } from "@/components/data-display";
import { workspaceFileUrl } from "@/lib/api/files";
import { downloadFileMux, readFileMux, writeFileMux } from "@/lib/mux-files";
import { cn } from "@/lib/utils";
import {
	Code,
	Download,
	ExternalLink,
	Eye,
	FileAudio,
	FileText,
	FileVideo,
	Loader2,
	Maximize2,
	Minimize2,
	Pencil,
	Save,
	X,
	ZoomIn,
	ZoomOut,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";

interface PreviewViewProps {
	filePath?: string | null;
	workspacePath?: string | null;
	/** Whether this is the default chat preview (uses different API) */
	isDefaultChat?: boolean;
	className?: string;
	onClose?: () => void;
	onToggleExpand?: () => void;
	isExpanded?: boolean;
	showExpand?: boolean;
	showHeader?: boolean;
}

// Simple LRU cache for file contents
const fileCache = new Map<string, { content: string; timestamp: number }>();
const CACHE_MAX_SIZE = 50;
const CACHE_TTL_MS = 30000; // 30 seconds

function getCachedContent(key: string): string | null {
	const entry = fileCache.get(key);
	if (!entry) return null;
	if (Date.now() - entry.timestamp > CACHE_TTL_MS) {
		fileCache.delete(key);
		return null;
	}
	return entry.content;
}

function setCachedContent(key: string, content: string) {
	// Evict oldest entries if cache is full
	if (fileCache.size >= CACHE_MAX_SIZE) {
		const oldestKey = fileCache.keys().next().value;
		if (oldestKey) fileCache.delete(oldestKey);
	}
	fileCache.set(key, { content, timestamp: Date.now() });
}

// File extensions that can be edited
const EDITABLE_EXTENSIONS = new Set([
	".txt",
	".md",
	".mdx",
	".markdown",
	".json",
	".jsonc",
	".yaml",
	".yml",
	".toml",
	".ini",
	".cfg",
	".conf",
	".env",
	".gitignore",
	".dockerignore",
	".js",
	".jsx",
	".ts",
	".tsx",
	".css",
	".scss",
	".sass",
	".less",
	".html",
	".htm",
	".xml",
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
]);

// Image extensions
const IMAGE_EXTENSIONS = new Set([
	".png",
	".jpg",
	".jpeg",
	".gif",
	".webp",
	".svg",
	".bmp",
	".ico",
]);

// PDF extension
const PDF_EXTENSIONS = new Set([".pdf"]);

// Video extensions
const VIDEO_EXTENSIONS = new Set([
	".mp4",
	".webm",
	".ogg",
	".ogv",
	".mov",
	".avi",
	".mkv",
	".m4v",
]);

// Audio extensions
const AUDIO_EXTENSIONS = new Set([
	".mp3",
	".wav",
	".flac",
	".aac",
	".m4a",
	".opus",
	".ogg",
]);

// Map file extensions to syntax highlighter language
function getLanguage(filename: string): string {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();

	const languageMap: Record<string, string> = {
		".js": "javascript",
		".jsx": "jsx",
		".ts": "typescript",
		".tsx": "tsx",
		".json": "json",
		".jsonc": "json",
		".md": "markdown",
		".mdx": "markdown",
		".markdown": "markdown",
		".py": "python",
		".rb": "ruby",
		".go": "go",
		".rs": "rust",
		".java": "java",
		".c": "c",
		".cpp": "cpp",
		".h": "c",
		".hpp": "cpp",
		".cs": "csharp",
		".php": "php",
		".swift": "swift",
		".kt": "kotlin",
		".scala": "scala",
		".html": "html",
		".htm": "html",
		".xml": "xml",
		".css": "css",
		".scss": "scss",
		".sass": "sass",
		".less": "less",
		".yaml": "yaml",
		".yml": "yaml",
		".toml": "toml",
		".ini": "ini",
		".cfg": "ini",
		".conf": "ini",
		".sh": "bash",
		".bash": "bash",
		".zsh": "bash",
		".fish": "bash",
		".sql": "sql",
		".graphql": "graphql",
		".vue": "vue",
		".svelte": "svelte",
		".txt": "text",
		".log": "text",
		".env": "bash",
		".gitignore": "text",
		".dockerignore": "text",
		".typ": "latex", // Typst uses latex highlighting as closest match
	};

	return languageMap[ext] || "text";
}

function isEditable(filename: string): boolean {
	const dotIndex = filename.lastIndexOf(".");
	if (dotIndex === -1) {
		return true;
	}
	const ext = filename.slice(dotIndex).toLowerCase();
	return EDITABLE_EXTENSIONS.has(ext);
}

function isImage(filename: string): boolean {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
	return IMAGE_EXTENSIONS.has(ext);
}

function isPdf(filename: string): boolean {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
	return PDF_EXTENSIONS.has(ext);
}

function isVideo(filename: string): boolean {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
	return VIDEO_EXTENSIONS.has(ext);
}

function isAudio(filename: string): boolean {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
	return AUDIO_EXTENSIONS.has(ext);
}

function getMediaMimeType(filename: string): string {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
	const mimeMap: Record<string, string> = {
		".mp4": "video/mp4",
		".webm": "video/webm",
		".ogg": "video/ogg",
		".ogv": "video/ogg",
		".mov": "video/quicktime",
		".avi": "video/x-msvideo",
		".mkv": "video/x-matroska",
		".m4v": "video/x-m4v",
		".mp3": "audio/mpeg",
		".wav": "audio/wav",
		".flac": "audio/flac",
		".aac": "audio/aac",
		".m4a": "audio/mp4",
		".opus": "audio/opus",
	};
	return mimeMap[ext] || "application/octet-stream";
}

async function fetchFileContent(
	workspacePath: string,
	path: string,
): Promise<string> {
	const result = await readFileMux(workspacePath, path);
	const decoder = new TextDecoder("utf-8");
	return decoder.decode(result.data);
}

async function saveFileContent(
	workspacePath: string,
	path: string,
	content: string,
): Promise<void> {
	const encoder = new TextEncoder();
	await writeFileMux(
		workspacePath,
		path,
		encoder.encode(content).buffer,
		false,
	);
}

export function PreviewView({
	filePath,
	workspacePath,
	isDefaultChat: _isDefaultChat = false,
	className,
	onClose,
	onToggleExpand,
	isExpanded = false,
	showExpand = false,
	showHeader = true,
}: PreviewViewProps) {
	const [content, setContent] = useState<string>("");
	const [editedContent, setEditedContent] = useState<string>("");
	const [showLoading, setShowLoading] = useState(false); // Delayed loading indicator
	const [saving, setSaving] = useState(false);
	const [error, setError] = useState<string>("");
	const [isEditing, setIsEditing] = useState(false);
	const [isDarkMode, setIsDarkMode] = useState(false);
	const [showMarkdownView, setShowMarkdownView] = useState(true);
	const [binaryUrl, setBinaryUrl] = useState<string | null>(null);
	const [binaryLoading, setBinaryLoading] = useState(false);
	const [mediaError, setMediaError] = useState<string>("");
	const loadingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const binaryObjectUrlRef = useRef<string | null>(null);
	const mediaBlobFallbackTriedRef = useRef(false);

	// Ref for scroll container to preserve scroll position when entering edit mode
	const scrollContainerRef = useRef<HTMLDivElement>(null);
	const savedScrollTopRef = useRef<number>(0);

	// Cache key: workspace path is sufficient for mux-only file reads
	const cacheKeyPrefix = workspacePath ?? null;

	// Detect dark mode
	useEffect(() => {
		const checkDarkMode = () => {
			setIsDarkMode(document.documentElement.classList.contains("dark"));
		};
		checkDarkMode();

		const observer = new MutationObserver(checkDarkMode);
		observer.observe(document.documentElement, {
			attributes: true,
			attributeFilter: ["class"],
		});

		return () => observer.disconnect();
	}, []);

	useEffect(() => {
		// Clear any pending loading timer
		if (loadingTimerRef.current) {
			clearTimeout(loadingTimerRef.current);
			loadingTimerRef.current = null;
		}

		if (!filePath || !workspacePath || !cacheKeyPrefix) {
			setContent("");
			setEditedContent("");
			setIsEditing(false);
			setShowLoading(false);
			return;
		}

		// Don't fetch content for PDF, image, video, or audio files - they render via URL
		const filename = filePath.split("/").pop() || filePath;
		if (
			isPdf(filename) ||
			isImage(filename) ||
			isVideo(filename) ||
			isAudio(filename)
		) {
			setContent("");
			setEditedContent("");
			setIsEditing(false);
			setShowLoading(false);
			return;
		}

		// Check cache first for instant preview
		const cacheKey = `${cacheKeyPrefix}:${filePath}`;
		const cached = getCachedContent(cacheKey);

		if (cached !== null) {
			setContent(cached);
			setEditedContent(cached);
			setError("");
			setIsEditing(false);
			setShowLoading(false);
			return;
		}

		// No cache hit - fetch from server
		// Only show loading spinner after 150ms to avoid flicker for fast responses
		setError("");
		setIsEditing(false);
		loadingTimerRef.current = setTimeout(() => {
			setShowLoading(true);
		}, 150);

		// Fetch raw content first (for editing)
		fetchFileContent(workspacePath, filePath)
			.then((data) => {
				// Cache and set raw content immediately
				setCachedContent(cacheKey, data);
				setContent(data);
				setEditedContent(data);
			})
			.catch((err) => {
				setError(err.message ?? "Failed to load file");
			})
			.finally(() => {
				if (loadingTimerRef.current) {
					clearTimeout(loadingTimerRef.current);
					loadingTimerRef.current = null;
				}
				setShowLoading(false);
			});

		return () => {
			if (loadingTimerRef.current) {
				clearTimeout(loadingTimerRef.current);
				loadingTimerRef.current = null;
			}
		};
	}, [filePath, workspacePath, cacheKeyPrefix]);

	useEffect(() => {
		if (binaryObjectUrlRef.current) {
			URL.revokeObjectURL(binaryObjectUrlRef.current);
			binaryObjectUrlRef.current = null;
		}
		mediaBlobFallbackTriedRef.current = false;
		setBinaryUrl(null);

		if (!filePath || !workspacePath) {
			setBinaryLoading(false);
			setMediaError("");
			return;
		}

		const filename = filePath.split("/").pop() || filePath;
		const isPdfFile = isPdf(filename);
		const isImageFile = isImage(filename);
		const isVideoFile = isVideo(filename);
		const isAudioFile = isAudio(filename);
		const isBinary = isPdfFile || isImageFile || isVideoFile || isAudioFile;
		if (!isBinary) {
			setBinaryLoading(false);
			setMediaError("");
			return;
		}

		// For video/audio prefer direct URL so the browser can use Range requests
		// for streaming/seek behavior instead of downloading full blob first.
		if (isVideoFile || isAudioFile) {
			const url = workspaceFileUrl(workspacePath, filePath);
			setBinaryUrl(url);
			setMediaError("");
			setBinaryLoading(false);
			return;
		}

		let cancelled = false;
		setBinaryLoading(true);
		setError("");
		setMediaError("");

		// Use WebSocket mux for binary file reads (PDFs, images) to avoid
		// dependency on the REST file proxy endpoint which requires a running
		// fileserver session.
		readFileMux(workspacePath, filePath)
			.then(({ data }) => {
				if (cancelled) return;
				// Determine MIME type from extension
				const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
				const mimeMap: Record<string, string> = {
					".pdf": "application/pdf",
					".png": "image/png",
					".jpg": "image/jpeg",
					".jpeg": "image/jpeg",
					".gif": "image/gif",
					".webp": "image/webp",
					".svg": "image/svg+xml",
					".bmp": "image/bmp",
					".ico": "image/x-icon",
				};
				const mimeType = mimeMap[ext] || "application/octet-stream";
				const blob = new Blob([data], { type: mimeType });
				const objectUrl = URL.createObjectURL(blob);
				binaryObjectUrlRef.current = objectUrl;
				setBinaryUrl(objectUrl);
			})
			.catch((err) => {
				if (cancelled) return;
				const fallbackUrl = workspaceFileUrl(workspacePath, filePath);
				setBinaryUrl(fallbackUrl);
				setMediaError(
					err instanceof Error
						? `Media mux read failed, using direct URL: ${err.message}`
						: "Media mux read failed, using direct URL",
				);
			})
			.finally(() => {
				if (!cancelled) setBinaryLoading(false);
			});

		return () => {
			cancelled = true;
			if (binaryObjectUrlRef.current) {
				URL.revokeObjectURL(binaryObjectUrlRef.current);
				binaryObjectUrlRef.current = null;
			}
		};
	}, [filePath, workspacePath]);

	const loadMediaBlobFallback = useCallback(async () => {
		if (!workspacePath || !filePath) return;
		if (mediaBlobFallbackTriedRef.current) return;
		mediaBlobFallbackTriedRef.current = true;

		setBinaryLoading(true);
		try {
			const filename = filePath.split("/").pop() || filePath;
			const { data } = await readFileMux(workspacePath, filePath);
			const mimeType = getMediaMimeType(filename);
			const blob = new Blob([data], { type: mimeType });
			if (binaryObjectUrlRef.current) {
				URL.revokeObjectURL(binaryObjectUrlRef.current);
			}
			const objectUrl = URL.createObjectURL(blob);
			binaryObjectUrlRef.current = objectUrl;
			setBinaryUrl(objectUrl);
			setMediaError("");
		} catch (err) {
			setMediaError(
				err instanceof Error
					? `Video/audio fallback failed: ${err.message}`
					: "Video/audio fallback failed",
			);
		} finally {
			setBinaryLoading(false);
		}
	}, [workspacePath, filePath]);

	const handleSave = useCallback(async () => {
		if (!workspacePath || !filePath || !cacheKeyPrefix) return;

		setSaving(true);
		setError("");
		try {
			await saveFileContent(workspacePath, filePath, editedContent);
			setContent(editedContent);
			// Update the cache with the new content
			const cacheKey = `${cacheKeyPrefix}:${filePath}`;
			setCachedContent(cacheKey, editedContent);
			setIsEditing(false);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to save file");
		} finally {
			setSaving(false);
		}
	}, [workspacePath, filePath, editedContent, cacheKeyPrefix]);

	const handleCancel = useCallback(() => {
		setEditedContent(content);
		setIsEditing(false);
		setError("");
	}, [content]);

	const handleStartEdit = useCallback(() => {
		// Save scroll position before entering edit mode
		if (scrollContainerRef.current) {
			savedScrollTopRef.current = scrollContainerRef.current.scrollTop;
		}
		setEditedContent(content);
		setIsEditing(true);
	}, [content]);

	// Restore scroll position after entering edit mode
	useEffect(() => {
		if (!isEditing || !scrollContainerRef.current) return;

		// Restore scroll position after editor mounts
		requestAnimationFrame(() => {
			if (scrollContainerRef.current) {
				scrollContainerRef.current.scrollTop = savedScrollTopRef.current;
			}
		});
	}, [isEditing]);

	// No file selected
	if (!filePath) {
		return (
			<div
				className={cn(
					"h-full bg-muted/30 rounded flex items-center justify-center",
					className,
				)}
			>
				<div className="text-center text-muted-foreground">
					<Eye className="w-12 h-12 mx-auto mb-2 opacity-50" />
					<p className="text-sm">No preview available</p>
					<p className="text-xs mt-1">Select a file to preview</p>
				</div>
			</div>
		);
	}

	// Loading state (only shown after delay to avoid flicker)
	if (showLoading || binaryLoading) {
		return (
			<div
				className={cn(
					"h-full bg-muted/30 rounded flex items-center justify-center",
					className,
				)}
			>
				<div className="text-center text-muted-foreground">
					<Loader2 className="w-8 h-8 mx-auto mb-2 animate-spin" />
					<p className="text-sm">Loading...</p>
				</div>
			</div>
		);
	}

	// Get filename from path
	const filename = filePath.split("/").pop() || filePath;
	const language = getLanguage(filename);
	const canEdit = isEditable(filename);
	const isImageFile = isImage(filename);
	const isPdfFile = isPdf(filename);
	const isVideoFile = isVideo(filename);
	const isAudioFile = isAudio(filename);

	// Check if file is markdown
	const isMarkdownFile =
		filename.toLowerCase().endsWith(".md") ||
		filename.toLowerCase().endsWith(".mdx") ||
		filename.toLowerCase().endsWith(".markdown");
	const fileUrl = binaryUrl;
	const imageUrl = isImageFile ? binaryUrl : null;
	const ExpandIcon = isExpanded ? Minimize2 : Maximize2;
	const expandLabel = isExpanded ? "Collapse preview" : "Expand preview";

	// For PDF files, render with iframe
	if (isPdfFile && fileUrl) {
		return (
			<div className={cn("h-full flex flex-col overflow-hidden", className)}>
				{/* Header */}
				{showHeader && (
					<div className="flex-shrink-0 flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
						<div className="flex items-center gap-1.5 flex-1 min-w-0">
							<FileText className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
							<p
								className="text-xs font-mono text-muted-foreground truncate"
								title={filePath}
							>
								{filename}
							</p>
						</div>
						<div className="flex items-center gap-0.5 ml-2">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => {
									if (fileUrl) {
										window.open(fileUrl, "_blank", "noopener");
									}
								}}
								className="h-6 px-1.5 text-xs"
								title="Open in new tab"
							>
								<ExternalLink className="w-3 h-3" />
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => {
									if (!workspacePath) return;
									void downloadFileMux(workspacePath, filePath, filename);
								}}
								className="h-6 px-1.5 text-xs"
								title="Download"
							>
								<Download className="w-3 h-3" />
							</Button>
							{showExpand && onToggleExpand && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={onToggleExpand}
									className="h-6 px-1.5 text-xs"
									title={expandLabel}
								>
									<ExpandIcon className="w-3 h-3" />
								</Button>
							)}
							{onClose && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={onClose}
									className="h-6 px-1.5 text-xs"
									title="Close preview"
								>
									<X className="w-3 h-3" />
								</Button>
							)}
						</div>
					</div>
				)}

				{/* PDF content */}
				<div className="flex-1 overflow-hidden bg-muted/30">
					<iframe
						src={fileUrl}
						className="w-full h-full border-0"
						title={filename}
					/>
				</div>
			</div>
		);
	}

	// For images, render a different view
	if (isImageFile) {
		return (
			<div className={cn("h-full flex flex-col overflow-hidden", className)}>
				{/* Header */}
				{showHeader && (
					<div className="flex-shrink-0 flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
						<p
							className="text-xs font-mono text-muted-foreground truncate flex-1"
							title={filePath}
						>
							{filename}
						</p>
						<div className="flex items-center gap-0.5 ml-2">
							{showExpand && onToggleExpand && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={onToggleExpand}
									className="h-6 px-1.5 text-xs"
									title={expandLabel}
								>
									<ExpandIcon className="w-3 h-3" />
								</Button>
							)}
							{onClose && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={onClose}
									className="h-6 px-1.5 text-xs"
									title="Close preview"
								>
									<X className="w-3 h-3" />
								</Button>
							)}
						</div>
					</div>
				)}

				{/* Image content */}
				<div className="flex-1 overflow-auto flex items-center justify-center p-4 bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMjAiIGhlaWdodD0iMjAiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+PGRlZnM+PHBhdHRlcm4gaWQ9ImdyaWQiIHdpZHRoPSIyMCIgaGVpZ2h0PSIyMCIgcGF0dGVyblVuaXRzPSJ1c2VyU3BhY2VPblVzZSI+PHJlY3QgZmlsbD0iIzgwODA4MCIgeD0iMCIgeT0iMCIgd2lkdGg9IjEwIiBoZWlnaHQ9IjEwIiBvcGFjaXR5PSIwLjEiLz48cmVjdCBmaWxsPSIjODA4MDgwIiB4PSIxMCIgeT0iMTAiIHdpZHRoPSIxMCIgaGVpZ2h0PSIxMCIgb3BhY2l0eT0iMC4xIi8+PC9wYXR0ZXJuPjwvZGVmcz48cmVjdCBmaWxsPSJ1cmwoI2dyaWQpIiB3aWR0aD0iMTAwJSIgaGVpZ2h0PSIxMDAlIi8+PC9zdmc+')]">
					{imageUrl && (
						<img
							src={imageUrl}
							alt={filename}
							onError={() => {
								const fallbackUrl = workspacePath
									? workspaceFileUrl(workspacePath, filePath)
									: null;
								if (fallbackUrl && imageUrl !== fallbackUrl) {
									setBinaryUrl(fallbackUrl);
								}
								setMediaError("Image preview failed to load. Trying fallback URL.");
							}}
							className="max-w-full max-h-full object-contain"
							style={{ imageRendering: "auto" }}
						/>
					)}
				</div>
				{mediaError && (
					<div className="px-3 py-2 text-[11px] text-amber-500 border-t border-border bg-muted/20">
						{mediaError}
					</div>
				)}
			</div>
		);
	}

	// For video files, render with video player
	if (isVideoFile && fileUrl) {
		return (
			<div className={cn("h-full flex flex-col overflow-hidden", className)}>
				{/* Header */}
				{showHeader && (
					<div className="flex-shrink-0 flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
						<div className="flex items-center gap-1.5 flex-1 min-w-0">
							<FileVideo className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
							<p
								className="text-xs font-mono text-muted-foreground truncate"
								title={filePath}
							>
								{filename}
							</p>
						</div>
						<div className="flex items-center gap-0.5 ml-2">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => {
									if (fileUrl) {
										window.open(fileUrl, "_blank", "noopener");
									}
								}}
								className="h-6 px-1.5 text-xs"
								title="Open in new tab"
							>
								<ExternalLink className="w-3 h-3" />
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => {
									if (!workspacePath) return;
									void downloadFileMux(workspacePath, filePath, filename);
								}}
								className="h-6 px-1.5 text-xs"
								title="Download"
							>
								<Download className="w-3 h-3" />
							</Button>
							{showExpand && onToggleExpand && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={onToggleExpand}
									className="h-6 px-1.5 text-xs"
									title={expandLabel}
								>
									<ExpandIcon className="w-3 h-3" />
								</Button>
							)}
							{onClose && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={onClose}
									className="h-6 px-1.5 text-xs"
									title="Close preview"
								>
									<X className="w-3 h-3" />
								</Button>
							)}
						</div>
					</div>
				)}

				{/* Video content */}
				<div className="flex-1 overflow-hidden bg-black flex items-center justify-center p-0 sm:p-3">
					<video
						controls
						playsInline
						preload="metadata"
						onError={() => {
							setMediaError(
								"Video playback failed in preview. Trying binary fallback...",
							);
							void loadMediaBlobFallback();
						}}
						className="w-full h-full object-contain"
					>
						<source src={fileUrl} type={getMediaMimeType(filename)} />
						<track kind="captions" />
						Your browser does not support the video tag.
					</video>
				</div>
				{mediaError && (
					<div className="px-3 py-2 text-[11px] text-amber-500 border-t border-border bg-muted/20">
						{mediaError}
					</div>
				)}
			</div>
		);
	}

	// For audio files, render with audio player
	if (isAudioFile && fileUrl) {
		return (
			<div className={cn("h-full flex flex-col overflow-hidden", className)}>
				{/* Header */}
				{showHeader && (
					<div className="flex-shrink-0 flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
						<div className="flex items-center gap-1.5 flex-1 min-w-0">
							<FileAudio className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
							<p
								className="text-xs font-mono text-muted-foreground truncate"
								title={filePath}
							>
								{filename}
							</p>
						</div>
						<div className="flex items-center gap-0.5 ml-2">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => {
									if (fileUrl) {
										window.open(fileUrl, "_blank", "noopener");
									}
								}}
								className="h-6 px-1.5 text-xs"
								title="Open in new tab"
							>
								<ExternalLink className="w-3 h-3" />
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => {
									if (!workspacePath) return;
									void downloadFileMux(workspacePath, filePath, filename);
								}}
								className="h-6 px-1.5 text-xs"
								title="Download"
							>
								<Download className="w-3 h-3" />
							</Button>
							{showExpand && onToggleExpand && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={onToggleExpand}
									className="h-6 px-1.5 text-xs"
									title={expandLabel}
								>
									<ExpandIcon className="w-3 h-3" />
								</Button>
							)}
							{onClose && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={onClose}
									className="h-6 px-1.5 text-xs"
									title="Close preview"
								>
									<X className="w-3 h-3" />
								</Button>
							)}
						</div>
					</div>
				)}

				{/* Audio content */}
				<div className="flex-1 overflow-hidden bg-muted/30 flex items-center justify-center p-4">
					{/* biome-ignore lint/a11y/useMediaCaption: audio files may not have captions */}
					<audio
						controls
						preload="metadata"
						onError={() => {
							setMediaError(
								"Audio playback failed in preview. Trying binary fallback...",
							);
							void loadMediaBlobFallback();
						}}
						className="w-full"
					>
						<source src={fileUrl} type={getMediaMimeType(filename)} />
						Your browser does not support the audio element.
					</audio>
				</div>
				{mediaError && (
					<div className="px-3 py-2 text-[11px] text-amber-500 border-t border-border bg-muted/20">
						{mediaError}
					</div>
				)}
			</div>
		);
	}

	return (
		<div className={cn("h-full flex flex-col overflow-hidden", className)}>
			{/* Header */}
			{showHeader && (
				<div
					className={cn(
						"flex-shrink-0 flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30",
						isExpanded && "pr-10",
					)}
				>
					<p
						className="text-xs font-mono text-muted-foreground truncate flex-1"
						title={filePath}
					>
						{filename}
						{isEditing && <span className="ml-2 text-primary">(editing)</span>}
					</p>
					<div className="flex items-center gap-0.5 ml-2">
						{isEditing ? (
							<>
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={handleCancel}
									disabled={saving}
									className="h-6 px-1.5 text-xs"
								>
									<X className="w-3 h-3 mr-1" />
									Cancel
								</Button>
								<Button
									type="button"
									variant="default"
									size="sm"
									onClick={handleSave}
									disabled={saving}
									className="h-6 px-1.5 text-xs"
								>
									{saving ? (
										<Loader2 className="w-3 h-3 mr-1 animate-spin" />
									) : (
										<Save className="w-3 h-3 mr-1" />
									)}
									Save
								</Button>
							</>
						) : (
							<>
								{isMarkdownFile && !isEditing && (
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={() => setShowMarkdownView((v) => !v)}
										className="h-6 px-1.5 text-xs"
										title={showMarkdownView ? "Show source" : "Show rendered"}
									>
										<Code className="w-3 h-3 mr-1" />
										{showMarkdownView ? "Code" : "Rendered"}
									</Button>
								)}
								{canEdit && (
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={handleStartEdit}
										className="h-6 px-1.5 text-xs"
									>
										<Pencil className="w-3 h-3 mr-1" />
										Edit
									</Button>
								)}
							</>
						)}
						{showExpand && onToggleExpand && (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={onToggleExpand}
								className="h-6 px-1.5 text-xs"
								title={expandLabel}
							>
								<ExpandIcon className="w-3 h-3" />
							</Button>
						)}
						{onClose && (
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={onClose}
								className="h-6 px-1.5 text-xs"
								title="Close preview"
							>
								<X className="w-3 h-3" />
							</Button>
						)}
					</div>
				</div>
			)}

			{/* Error message */}
			{error && (
				<div className="flex-shrink-0 px-3 py-2 bg-destructive/10 text-destructive text-xs">
					{error}
				</div>
			)}

			{/* Content */}
			<div ref={scrollContainerRef} className="flex-1 overflow-auto">
				{isEditing ? (
					<textarea
						value={editedContent}
						onChange={(e) => setEditedContent(e.target.value)}
						spellCheck={false}
						autoComplete="off"
						autoCorrect="off"
						autoCapitalize="off"
						className="w-full h-full resize-none outline-none"
						style={{
							fontSize: 12,
							fontFamily:
								"ui-monospace, SFMono-Regular, SF Mono, Consolas, Liberation Mono, Menlo, monospace",
							minHeight: "100%",
							padding: 12,
							backgroundColor: isDarkMode ? "#1e1e1e" : "#ffffff",
							color: isDarkMode ? "#d4d4d4" : "#1e1e1e",
							border: "none",
						}}
					/>
				) : content ? (
					isMarkdownFile && showMarkdownView ? (
						<div className="p-4" style={{ minHeight: "100%" }}>
							<MarkdownRenderer
								content={content}
								className="text-sm leading-relaxed"
							/>
						</div>
					) : (
						<div
							className="overflow-auto py-1"
							style={{
								minHeight: "100%",
							}}
						>
							<SyntaxHighlighter
								style={oneDark as Record<string, React.CSSProperties>}
								language={language}
								showLineNumbers
								wrapLines
								wrapLongLines
								customStyle={{
									margin: 0,
									padding: "12px",
									background: "transparent",
									fontSize: "12px",
									minHeight: "100%",
								}}
								lineNumberStyle={{
									minWidth: "2.5em",
									paddingRight: "1em",
									color: "var(--muted-foreground)",
									opacity: 0.5,
								}}
							>
								{content}
							</SyntaxHighlighter>
						</div>
					)
				) : null}
			</div>
		</div>
	);
}
