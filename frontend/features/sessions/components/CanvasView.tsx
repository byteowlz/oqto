"use client";

import { Button } from "@/components/ui/button";
import { fileserverWorkspaceBaseUrl } from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import type Konva from "konva";
import {
	ArrowRight,
	Circle,
	Download,
	Eraser,
	Hand,
	Highlighter,
	MessageSquarePlus,
	MousePointer2,
	Pencil,
	Redo2,
	Save,
	Square,
	Trash2,
	Type,
	Undo2,
	Upload,
	ZoomIn,
	ZoomOut,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
	Arrow,
	Group,
	Circle as KonvaCircle,
	Image as KonvaImage,
	Layer,
	Line,
	Rect,
	Stage,
	Text,
	Transformer,
} from "react-konva";

interface CanvasViewProps {
	workspacePath?: string | null;
	initialImagePath?: string | null;
	className?: string;
	/** Called when user clicks "Save & Add to Chat" - provides the saved file path */
	onSaveAndAddToChat?: (filePath: string) => void;
}

type Tool =
	| "select"
	| "pan"
	| "pencil"
	| "arrow"
	| "rect"
	| "circle"
	| "text"
	| "highlighter"
	| "eraser";

interface AnnotationBase {
	id: string;
	type: string;
	x: number;
	y: number;
}

interface LineAnnotation extends AnnotationBase {
	type: "line" | "highlighter";
	points: number[];
	stroke: string;
	strokeWidth: number;
	opacity?: number;
}

interface ArrowAnnotation extends AnnotationBase {
	type: "arrow";
	points: number[];
	stroke: string;
	strokeWidth: number;
}

interface RectAnnotation extends AnnotationBase {
	type: "rect";
	width: number;
	height: number;
	stroke: string;
	strokeWidth: number;
	fill?: string;
}

interface CircleAnnotation extends AnnotationBase {
	type: "circle";
	radius: number;
	stroke: string;
	strokeWidth: number;
	fill?: string;
}

interface TextAnnotation extends AnnotationBase {
	type: "text";
	text: string;
	fontSize: number;
	fill: string;
}

interface ImageAnnotation extends AnnotationBase {
	type: "image";
	width: number;
	height: number;
	src: string;
}

type Annotation =
	| LineAnnotation
	| ArrowAnnotation
	| RectAnnotation
	| CircleAnnotation
	| TextAnnotation
	| ImageAnnotation;

// Generate unique ID
function generateId(): string {
	return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

// Canvas state cache - persists across component unmounts
interface CanvasStateCache {
	annotations: Annotation[];
	history: Annotation[][];
	historyIndex: number;
	scale: number;
	position: { x: number; y: number };
	tool: Tool;
	color: string;
	strokeWidth: number;
	fontSize: number;
	backgroundImageSrc: string | null;
	backgroundSize: { width: number; height: number };
	// Track which image path this cache is for
	initialImagePath: string | null;
}

const canvasCache: { state: CanvasStateCache | null } = { state: null };

function saveCanvasState(state: CanvasStateCache) {
	canvasCache.state = state;
}

function loadCanvasState(
	initialImagePath: string | null,
): CanvasStateCache | null {
	const cached = canvasCache.state;
	// Only return cache if it matches the current image path
	// This ensures "Open in Canvas" with a new image gets a fresh canvas
	if (cached && cached.initialImagePath === initialImagePath) {
		return cached;
	}
	return null;
}

function clearCanvasCache() {
	canvasCache.state = null;
}

// Tool button component
const ToolButton = memo(function ToolButton({
	tool,
	currentTool,
	onSelect,
	icon: Icon,
	title,
}: {
	tool: Tool;
	currentTool: Tool;
	onSelect: (tool: Tool) => void;
	icon: typeof Pencil;
	title: string;
}) {
	return (
		<Button
			type="button"
			variant={currentTool === tool ? "default" : "ghost"}
			size="sm"
			onClick={() => onSelect(tool)}
			className="h-8 w-8 p-0"
			title={title}
		>
			<Icon className="w-4 h-4" />
		</Button>
	);
});

// Annotation shape renderer
const AnnotationShape = memo(function AnnotationShape({
	annotation,
	isSelected,
	onSelect,
	onChange,
	image,
}: {
	annotation: Annotation;
	isSelected: boolean;
	onSelect: () => void;
	onChange: (newAttrs: Partial<Annotation>) => void;
	image?: HTMLImageElement | null;
}) {
	const shapeRef = useRef<Konva.Shape>(null);

	const handleDragEnd = useCallback(
		(e: Konva.KonvaEventObject<DragEvent>) => {
			onChange({
				x: e.target.x(),
				y: e.target.y(),
			});
		},
		[onChange],
	);

	switch (annotation.type) {
		case "line":
		case "highlighter":
			return (
				<Line
					ref={shapeRef as React.RefObject<Konva.Line>}
					x={annotation.x}
					y={annotation.y}
					points={annotation.points}
					stroke={annotation.stroke}
					strokeWidth={annotation.strokeWidth}
					opacity={annotation.opacity ?? 1}
					lineCap="round"
					lineJoin="round"
					draggable
					onClick={onSelect}
					onTap={onSelect}
					onDragEnd={handleDragEnd}
				/>
			);
		case "arrow":
			return (
				<Arrow
					ref={shapeRef as React.RefObject<Konva.Arrow>}
					x={annotation.x}
					y={annotation.y}
					points={annotation.points}
					stroke={annotation.stroke}
					strokeWidth={annotation.strokeWidth}
					fill={annotation.stroke}
					pointerLength={10}
					pointerWidth={10}
					draggable
					onClick={onSelect}
					onTap={onSelect}
					onDragEnd={handleDragEnd}
				/>
			);
		case "rect":
			return (
				<Rect
					ref={shapeRef as React.RefObject<Konva.Rect>}
					x={annotation.x}
					y={annotation.y}
					width={annotation.width}
					height={annotation.height}
					stroke={annotation.stroke}
					strokeWidth={annotation.strokeWidth}
					fill={annotation.fill}
					draggable
					onClick={onSelect}
					onTap={onSelect}
					onDragEnd={handleDragEnd}
				/>
			);
		case "circle":
			return (
				<KonvaCircle
					ref={shapeRef as React.RefObject<Konva.Circle>}
					x={annotation.x}
					y={annotation.y}
					radius={annotation.radius}
					stroke={annotation.stroke}
					strokeWidth={annotation.strokeWidth}
					fill={annotation.fill}
					draggable
					onClick={onSelect}
					onTap={onSelect}
					onDragEnd={handleDragEnd}
				/>
			);
		case "text":
			return (
				<Text
					ref={shapeRef as React.RefObject<Konva.Text>}
					x={annotation.x}
					y={annotation.y}
					text={annotation.text}
					fontSize={annotation.fontSize}
					fill={annotation.fill}
					draggable
					onClick={onSelect}
					onTap={onSelect}
					onDragEnd={handleDragEnd}
				/>
			);
		case "image":
			if (!image) return null;
			return (
				<KonvaImage
					ref={shapeRef as React.RefObject<Konva.Image>}
					x={annotation.x}
					y={annotation.y}
					width={annotation.width}
					height={annotation.height}
					image={image}
					draggable
					onClick={onSelect}
					onTap={onSelect}
					onDragEnd={handleDragEnd}
				/>
			);
		default:
			return null;
	}
});

export const CanvasView = memo(function CanvasView({
	workspacePath,
	initialImagePath,
	className,
	onSaveAndAddToChat,
}: CanvasViewProps) {
	// Track current initialImagePath to detect changes
	const prevInitialImagePathRef = useRef<string | null>(
		initialImagePath ?? null,
	);

	// Load cached state on mount - restore cache if it exists (regardless of initialImagePath match)
	// This ensures canvas content survives expand/collapse
	const getCachedState = () => {
		// First try to load cache matching current initialImagePath
		const matchingCache = loadCanvasState(initialImagePath ?? null);
		if (matchingCache) return matchingCache;

		// If no matching cache but we have a generic cache, use it for expand/collapse persistence
		// Only do this if initialImagePath is null/undefined (canvas view without specific file)
		if (!initialImagePath && canvasCache.state) {
			return canvasCache.state;
		}
		return null;
	};
	const cached = getCachedState();

	// Canvas state - initialize from cache if available
	const [tool, setTool] = useState<Tool>(cached?.tool ?? "select");
	const [color, setColor] = useState(cached?.color ?? "#ff0000");
	const [strokeWidth, setStrokeWidth] = useState(cached?.strokeWidth ?? 3);
	const [fontSize, setFontSize] = useState(cached?.fontSize ?? 16);
	const [annotations, setAnnotations] = useState<Annotation[]>(
		cached?.annotations ?? [],
	);
	const [selectedId, setSelectedId] = useState<string | null>(null);
	const [history, setHistory] = useState<Annotation[][]>(
		cached?.history ?? [[]],
	);
	const [historyIndex, setHistoryIndex] = useState(cached?.historyIndex ?? 0);
	const [scale, setScale] = useState(cached?.scale ?? 1);
	const [position, setPosition] = useState(cached?.position ?? { x: 0, y: 0 });

	// Background image state
	const [backgroundImage, setBackgroundImage] =
		useState<HTMLImageElement | null>(null);
	const [backgroundImageSrc, setBackgroundImageSrc] = useState<string | null>(
		cached?.backgroundImageSrc ?? null,
	);
	const [backgroundSize, setBackgroundSize] = useState(
		cached?.backgroundSize ?? { width: 800, height: 600 },
	);

	// Pasted/loaded images cache
	const [imageCache, setImageCache] = useState<Map<string, HTMLImageElement>>(
		new Map(),
	);

	// Restore background image from cached src
	useEffect(() => {
		if (backgroundImageSrc && !backgroundImage) {
			const img = new Image();
			img.onload = () => setBackgroundImage(img);
			img.src = backgroundImageSrc;
		}
	}, [backgroundImageSrc, backgroundImage]);

	// Restore image annotations from cache on mount
	const initialAnnotations = cached?.annotations;
	useEffect(() => {
		if (initialAnnotations) {
			for (const ann of initialAnnotations) {
				if (ann.type === "image") {
					const img = new Image();
					img.onload = () => {
						setImageCache((prev) => new Map(prev).set(ann.id, img));
					};
					img.src = ann.src;
				}
			}
		}
	}, [initialAnnotations]);

	// Persist state to cache so it survives remounts (expand/collapse).
	useEffect(() => {
		saveCanvasState({
			annotations,
			history,
			historyIndex,
			scale,
			position,
			tool,
			color,
			strokeWidth,
			fontSize,
			backgroundImageSrc,
			backgroundSize,
			initialImagePath: initialImagePath ?? null,
		});
	}, [
		annotations,
		history,
		historyIndex,
		scale,
		position,
		tool,
		color,
		strokeWidth,
		fontSize,
		backgroundImageSrc,
		backgroundSize,
		initialImagePath,
	]);

	// Drawing state
	const [isDrawing, setIsDrawing] = useState(false);
	const [currentAnnotation, setCurrentAnnotation] = useState<Annotation | null>(
		null,
	);
	const [textInput, setTextInput] = useState("");
	const [textInputPosition, setTextInputPosition] = useState<{
		x: number;
		y: number;
	} | null>(null);

	// Refs
	const stageRef = useRef<Konva.Stage>(null);
	const containerRef = useRef<HTMLDivElement>(null);
	const transformerRef = useRef<Konva.Transformer>(null);
	const fileInputRef = useRef<HTMLInputElement>(null);
	const textInputRef = useRef<HTMLInputElement>(null);

	// Detect dark mode for canvas background
	// Initialize with SSR-safe check to avoid flash of white
	const [isDarkMode, setIsDarkMode] = useState(() => {
		if (typeof document !== "undefined") {
			return document.documentElement.classList.contains("dark");
		}
		return false;
	});
	useEffect(() => {
		const checkDarkMode = () => {
			setIsDarkMode(document.documentElement.classList.contains("dark"));
		};
		checkDarkMode();
		// Watch for class changes on html element
		const observer = new MutationObserver(checkDarkMode);
		observer.observe(document.documentElement, {
			attributes: true,
			attributeFilter: ["class"],
		});
		return () => observer.disconnect();
	}, []);

	const canvasBackgroundColor = isDarkMode ? "#2d312f" : "#ffffff"; // --card color for dark

	const fileserverBaseUrl = workspacePath ? fileserverWorkspaceBaseUrl() : null;

	// Detect when initialImagePath changes (e.g., "Open in Canvas" with a new file)
	// and reset canvas state to load the new image
	useEffect(() => {
		const prevPath = prevInitialImagePathRef.current;
		const currentPath = initialImagePath ?? null;

		// If path changed and we have a new image to load
		if (prevPath !== currentPath && currentPath) {
			// Clear current background to force reload
			setBackgroundImage(null);
			setBackgroundImageSrc(null);
			// Clear annotations for fresh canvas with new image
			setAnnotations([]);
			setHistory([[]]);
			setHistoryIndex(0);
			setScale(1);
			setPosition({ x: 0, y: 0 });
			setImageCache(new Map());
		}

		prevInitialImagePathRef.current = currentPath;
	}, [initialImagePath]);

	// Load initial image if provided - this takes priority over cached state
	useEffect(() => {
		if (!initialImagePath || !fileserverBaseUrl || !workspacePath) return;

		const url = new URL(`${fileserverBaseUrl}/file`, window.location.origin);
		url.searchParams.set("path", initialImagePath);
		url.searchParams.set("workspace_path", workspacePath);

		const imgSrc = url.toString();

		// Skip if already showing this image
		if (backgroundImageSrc === imgSrc) return;

		const img = new Image();
		img.onload = () => {
			setBackgroundImage(img);
			setBackgroundImageSrc(imgSrc);
			setBackgroundSize({ width: img.width, height: img.height });
		};
		img.src = imgSrc;
	}, [initialImagePath, fileserverBaseUrl, workspacePath, backgroundImageSrc]);

	// Handle paste events for images
	useEffect(() => {
		const handlePaste = async (e: ClipboardEvent) => {
			const items = e.clipboardData?.items;
			if (!items) return;

			for (const item of items) {
				if (item.type.startsWith("image/")) {
					e.preventDefault();
					const blob = item.getAsFile();
					if (!blob) continue;

					const reader = new FileReader();
					reader.onload = (event) => {
						const dataUrl = event.target?.result as string;
						const img = new Image();
						img.onload = () => {
							// If no background, use as background
							if (!backgroundImage) {
								setBackgroundImage(img);
								setBackgroundImageSrc(dataUrl);
								setBackgroundSize({ width: img.width, height: img.height });
							} else {
								// Add as annotation
								const id = generateId();
								const newAnnotation: ImageAnnotation = {
									id,
									type: "image",
									x: 50,
									y: 50,
									width: img.width,
									height: img.height,
									src: dataUrl,
								};
								setImageCache((prev) => new Map(prev).set(id, img));
								addAnnotation(newAnnotation);
							}
						};
						img.src = dataUrl;
					};
					reader.readAsDataURL(blob);
					break;
				}
			}
		};

		window.addEventListener("paste", handlePaste);
		return () => window.removeEventListener("paste", handlePaste);
	}, [backgroundImage]);

	// History management
	const addToHistory = useCallback(
		(newAnnotations: Annotation[]) => {
			setHistory((prev) => {
				const newHistory = prev.slice(0, historyIndex + 1);
				newHistory.push([...newAnnotations]);
				return newHistory;
			});
			setHistoryIndex((prev) => prev + 1);
		},
		[historyIndex],
	);

	const addAnnotation = useCallback(
		(annotation: Annotation) => {
			const newAnnotations = [...annotations, annotation];
			setAnnotations(newAnnotations);
			addToHistory(newAnnotations);
		},
		[annotations, addToHistory],
	);

	// Handle file upload
	const handleFileUpload = useCallback(
		(e: React.ChangeEvent<HTMLInputElement>) => {
			const file = e.target.files?.[0];
			if (!file || !file.type.startsWith("image/")) return;

			const reader = new FileReader();
			reader.onload = (event) => {
				const dataUrl = event.target?.result as string;
				const img = new Image();
				img.onload = () => {
					// If no background, use as background
					if (!backgroundImage) {
						setBackgroundImage(img);
						setBackgroundImageSrc(dataUrl);
						setBackgroundSize({ width: img.width, height: img.height });
					} else {
						// Add as annotation
						const id = generateId();
						const newAnnotation: ImageAnnotation = {
							id,
							type: "image",
							x: 50,
							y: 50,
							width: img.width,
							height: img.height,
							src: dataUrl,
						};
						setImageCache((prev) => new Map(prev).set(id, img));
						addAnnotation(newAnnotation);
					}
				};
				img.src = dataUrl;
			};
			reader.readAsDataURL(file);

			// Reset input so same file can be uploaded again
			e.target.value = "";
		},
		[backgroundImage, addAnnotation],
	);

	const updateAnnotation = useCallback(
		(id: string, newAttrs: Partial<Annotation>) => {
			const newAnnotations = annotations.map((a) =>
				a.id === id ? { ...a, ...newAttrs } : a,
			);
			setAnnotations(newAnnotations);
		},
		[annotations],
	);

	const deleteAnnotation = useCallback(
		(id: string) => {
			const newAnnotations = annotations.filter((a) => a.id !== id);
			setAnnotations(newAnnotations);
			addToHistory(newAnnotations);
			setSelectedId(null);
		},
		[annotations, addToHistory],
	);

	const undo = useCallback(() => {
		if (historyIndex > 0) {
			setHistoryIndex((prev) => prev - 1);
			setAnnotations([...history[historyIndex - 1]]);
			setSelectedId(null);
		}
	}, [history, historyIndex]);

	const redo = useCallback(() => {
		if (historyIndex < history.length - 1) {
			setHistoryIndex((prev) => prev + 1);
			setAnnotations([...history[historyIndex + 1]]);
			setSelectedId(null);
		}
	}, [history, historyIndex]);

	// Clear all
	const clearAll = useCallback(() => {
		setAnnotations([]);
		addToHistory([]);
		setSelectedId(null);
		setBackgroundImage(null);
		setBackgroundImageSrc(null);
		setImageCache(new Map());
		clearCanvasCache();
	}, [addToHistory]);

	// Zoom controls
	const zoomIn = useCallback(() => {
		setScale((prev) => Math.min(prev * 1.2, 5));
	}, []);

	const zoomOut = useCallback(() => {
		setScale((prev) => Math.max(prev / 1.2, 0.1));
	}, []);

	const resetZoom = useCallback(() => {
		setScale(1);
		setPosition({ x: 0, y: 0 });
	}, []);

	// Mouse/touch handlers
	const handleMouseDown = useCallback(
		(e: Konva.KonvaEventObject<MouseEvent | TouchEvent>) => {
			if (tool === "select" || tool === "pan") {
				// Check if clicked on empty area
				const clickedOnEmpty = e.target === e.target.getStage();
				if (clickedOnEmpty) {
					setSelectedId(null);
				}
				return;
			}

			const stage = e.target.getStage();
			if (!stage) return;

			const pos = stage.getPointerPosition();
			if (!pos) return;

			// Adjust for scale and position
			const adjustedPos = {
				x: (pos.x - position.x) / scale,
				y: (pos.y - position.y) / scale,
			};

			setIsDrawing(true);

			const id = generateId();

			switch (tool) {
				case "pencil":
					setCurrentAnnotation({
						id,
						type: "line",
						x: 0,
						y: 0,
						points: [adjustedPos.x, adjustedPos.y],
						stroke: color,
						strokeWidth,
					});
					break;
				case "highlighter":
					setCurrentAnnotation({
						id,
						type: "highlighter",
						x: 0,
						y: 0,
						points: [adjustedPos.x, adjustedPos.y],
						stroke: "#ffff00",
						strokeWidth: 20,
						opacity: 0.4,
					});
					break;
				case "arrow":
					setCurrentAnnotation({
						id,
						type: "arrow",
						x: 0,
						y: 0,
						points: [
							adjustedPos.x,
							adjustedPos.y,
							adjustedPos.x,
							adjustedPos.y,
						],
						stroke: color,
						strokeWidth,
					});
					break;
				case "rect":
					setCurrentAnnotation({
						id,
						type: "rect",
						x: adjustedPos.x,
						y: adjustedPos.y,
						width: 0,
						height: 0,
						stroke: color,
						strokeWidth,
					});
					break;
				case "circle":
					setCurrentAnnotation({
						id,
						type: "circle",
						x: adjustedPos.x,
						y: adjustedPos.y,
						radius: 0,
						stroke: color,
						strokeWidth,
					});
					break;
				case "text":
					setTextInputPosition(adjustedPos);
					setIsDrawing(false);
					break;
				case "eraser": {
					// Find and delete annotation at position
					const annotations_copy = [...annotations];
					for (let i = annotations_copy.length - 1; i >= 0; i--) {
						const ann = annotations_copy[i];
						// Simple hit detection - check if point is near annotation
						const dx = adjustedPos.x - ann.x;
						const dy = adjustedPos.y - ann.y;
						const distance = Math.sqrt(dx * dx + dy * dy);
						if (distance < 20) {
							deleteAnnotation(ann.id);
							break;
						}
					}
					setIsDrawing(false);
					break;
				}
			}
		},
		[tool, color, strokeWidth, scale, position, annotations, deleteAnnotation],
	);

	const handleMouseMove = useCallback(
		(e: Konva.KonvaEventObject<MouseEvent | TouchEvent>) => {
			if (!isDrawing || !currentAnnotation) return;

			const stage = e.target.getStage();
			if (!stage) return;

			const pos = stage.getPointerPosition();
			if (!pos) return;

			const adjustedPos = {
				x: (pos.x - position.x) / scale,
				y: (pos.y - position.y) / scale,
			};

			switch (currentAnnotation.type) {
				case "line":
				case "highlighter":
					setCurrentAnnotation({
						...currentAnnotation,
						points: [...currentAnnotation.points, adjustedPos.x, adjustedPos.y],
					});
					break;
				case "arrow":
					setCurrentAnnotation({
						...currentAnnotation,
						points: [
							currentAnnotation.points[0],
							currentAnnotation.points[1],
							adjustedPos.x,
							adjustedPos.y,
						],
					});
					break;
				case "rect":
					setCurrentAnnotation({
						...currentAnnotation,
						width: adjustedPos.x - currentAnnotation.x,
						height: adjustedPos.y - currentAnnotation.y,
					});
					break;
				case "circle": {
					const dx = adjustedPos.x - currentAnnotation.x;
					const dy = adjustedPos.y - currentAnnotation.y;
					setCurrentAnnotation({
						...currentAnnotation,
						radius: Math.sqrt(dx * dx + dy * dy),
					});
					break;
				}
			}
		},
		[isDrawing, currentAnnotation, scale, position],
	);

	const handleMouseUp = useCallback(() => {
		if (isDrawing && currentAnnotation) {
			addAnnotation(currentAnnotation);
			setCurrentAnnotation(null);
		}
		setIsDrawing(false);
	}, [isDrawing, currentAnnotation, addAnnotation]);

	// Add text annotation
	const handleTextSubmit = useCallback(() => {
		if (!textInputPosition || !textInput.trim()) {
			setTextInputPosition(null);
			setTextInput("");
			return;
		}

		const annotation: TextAnnotation = {
			id: generateId(),
			type: "text",
			x: textInputPosition.x,
			y: textInputPosition.y,
			text: textInput,
			fontSize,
			fill: color,
		};
		addAnnotation(annotation);
		setTextInputPosition(null);
		setTextInput("");
	}, [textInputPosition, textInput, color, fontSize, addAnnotation]);

	// Save canvas as image
	const saveAsImage = useCallback(async () => {
		if (!stageRef.current) return;

		const dataUrl = stageRef.current.toDataURL({ pixelRatio: 2 });

		// Create download link
		const link = document.createElement("a");
		link.download = `canvas-${Date.now()}.png`;
		link.href = dataUrl;
		link.click();
	}, []);

	// Save to workspace - returns the filename if successful
	const saveToWorkspace = useCallback(async (): Promise<string | null> => {
		if (!stageRef.current || !fileserverBaseUrl || !workspacePath) return null;

		const dataUrl = stageRef.current.toDataURL({ pixelRatio: 2 });

		// Convert data URL to blob
		const response = await fetch(dataUrl);
		const blob = await response.blob();

		// Create form data
		const formData = new FormData();
		const filename = `annotated-${Date.now()}.png`;
		formData.append("file", blob, filename);

		// Upload to workspace
		const url = new URL(`${fileserverBaseUrl}/file`, window.location.origin);
		url.searchParams.set("path", filename);
		url.searchParams.set("workspace_path", workspacePath);

		try {
			const res = await fetch(url.toString(), {
				method: "POST",
				credentials: "include",
				body: formData,
			});

			if (!res.ok) {
				throw new Error("Failed to save image");
			}

			console.log("Saved to workspace:", filename);
			return filename;
		} catch (err) {
			console.error("Failed to save:", err);
			return null;
		}
	}, [fileserverBaseUrl, workspacePath]);

	// Save and add to chat
	const saveAndAddToChat = useCallback(async () => {
		const filename = await saveToWorkspace();
		if (filename && onSaveAndAddToChat) {
			onSaveAndAddToChat(filename);
		}
	}, [saveToWorkspace, onSaveAndAddToChat]);

	// Focus text input when it appears
	useEffect(() => {
		if (textInputPosition && textInputRef.current) {
			// Small delay to ensure DOM is ready
			setTimeout(() => {
				textInputRef.current?.focus();
			}, 10);
		}
	}, [textInputPosition]);

	// Keyboard shortcuts
	useEffect(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			// Skip all shortcuts when text input is active
			if (textInputPosition) return;

			// Ignore shortcuts when typing in an input or textarea
			const target = e.target as HTMLElement;
			const isTyping =
				target.tagName === "INPUT" ||
				target.tagName === "TEXTAREA" ||
				target.isContentEditable;

			// Delete selected annotation (but not when typing)
			if (
				(e.key === "Delete" || e.key === "Backspace") &&
				selectedId &&
				!isTyping
			) {
				deleteAnnotation(selectedId);
			}
			// Undo/Redo - allow even when typing
			if (e.ctrlKey || e.metaKey) {
				if (e.key === "z" && !e.shiftKey) {
					e.preventDefault();
					undo();
				}
				if ((e.key === "z" && e.shiftKey) || e.key === "y") {
					e.preventDefault();
					redo();
				}
			}
			// Tool shortcuts - skip when typing
			if (!e.ctrlKey && !e.metaKey && !isTyping) {
				switch (e.key) {
					case "v":
						setTool("select");
						break;
					case "h":
						setTool("pan");
						break;
					case "p":
						setTool("pencil");
						break;
					case "a":
						setTool("arrow");
						break;
					case "r":
						setTool("rect");
						break;
					case "c":
						setTool("circle");
						break;
					case "t":
						setTool("text");
						break;
					case "e":
						setTool("eraser");
						break;
				}
			}
		};

		window.addEventListener("keydown", handleKeyDown);
		return () => window.removeEventListener("keydown", handleKeyDown);
	}, [selectedId, deleteAnnotation, undo, redo, textInputPosition]);

	// Container size
	const [containerSize, setContainerSize] = useState({
		width: 800,
		height: 600,
	});

	useEffect(() => {
		const updateSize = () => {
			if (containerRef.current) {
				setContainerSize({
					width: containerRef.current.offsetWidth,
					height: containerRef.current.offsetHeight,
				});
			}
		};

		updateSize();
		window.addEventListener("resize", updateSize);
		return () => window.removeEventListener("resize", updateSize);
	}, []);

	// Calculate canvas size
	const canvasWidth = backgroundImage
		? backgroundSize.width
		: containerSize.width;
	const canvasHeight = backgroundImage
		? backgroundSize.height
		: containerSize.height;

	return (
		<div
			className={cn("flex flex-col h-full overflow-hidden", className)}
			data-spotlight="canvas"
		>
			{/* Toolbar */}
			<div className="flex-shrink-0 flex items-center gap-1 p-2 border-b border-border bg-muted/30 flex-wrap">
				{/* Tool buttons */}
				<div className="flex items-center gap-0.5 mr-2">
					<ToolButton
						tool="select"
						currentTool={tool}
						onSelect={setTool}
						icon={MousePointer2}
						title="Select (V)"
					/>
					<ToolButton
						tool="pan"
						currentTool={tool}
						onSelect={setTool}
						icon={Hand}
						title="Pan (H)"
					/>
					<ToolButton
						tool="pencil"
						currentTool={tool}
						onSelect={setTool}
						icon={Pencil}
						title="Pencil (P)"
					/>
					<ToolButton
						tool="arrow"
						currentTool={tool}
						onSelect={setTool}
						icon={ArrowRight}
						title="Arrow (A)"
					/>
					<ToolButton
						tool="rect"
						currentTool={tool}
						onSelect={setTool}
						icon={Square}
						title="Rectangle (R)"
					/>
					<ToolButton
						tool="circle"
						currentTool={tool}
						onSelect={setTool}
						icon={Circle}
						title="Circle (C)"
					/>
					<ToolButton
						tool="text"
						currentTool={tool}
						onSelect={setTool}
						icon={Type}
						title="Text (T)"
					/>
					<ToolButton
						tool="highlighter"
						currentTool={tool}
						onSelect={setTool}
						icon={Highlighter}
						title="Highlighter"
					/>
					<ToolButton
						tool="eraser"
						currentTool={tool}
						onSelect={setTool}
						icon={Eraser}
						title="Eraser (E)"
					/>
				</div>

				{/* Color picker */}
				<div className="flex items-center gap-1 mr-2">
					<input
						type="color"
						value={color}
						onChange={(e) => setColor(e.target.value)}
						className="w-8 h-8 rounded cursor-pointer border border-border"
						title="Color"
					/>
					<select
						value={strokeWidth}
						onChange={(e) => setStrokeWidth(Number(e.target.value))}
						className="h-8 text-xs bg-background border border-border rounded px-1"
						title="Stroke width"
					>
						<option value={1}>1px</option>
						<option value={2}>2px</option>
						<option value={3}>3px</option>
						<option value={5}>5px</option>
						<option value={8}>8px</option>
					</select>
					<select
						value={fontSize}
						onChange={(e) => setFontSize(Number(e.target.value))}
						className="h-8 text-xs bg-background border border-border rounded px-1"
						title="Font size"
					>
						<option value={12}>12pt</option>
						<option value={14}>14pt</option>
						<option value={16}>16pt</option>
						<option value={20}>20pt</option>
						<option value={24}>24pt</option>
						<option value={32}>32pt</option>
						<option value={48}>48pt</option>
					</select>
				</div>

				{/* History buttons */}
				<div className="flex items-center gap-0.5 mr-2">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={undo}
						disabled={historyIndex <= 0}
						className="h-8 w-8 p-0"
						title="Undo (Ctrl+Z)"
					>
						<Undo2 className="w-4 h-4" />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={redo}
						disabled={historyIndex >= history.length - 1}
						className="h-8 w-8 p-0"
						title="Redo (Ctrl+Y)"
					>
						<Redo2 className="w-4 h-4" />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={clearAll}
						className="h-8 w-8 p-0"
						title="Clear all"
					>
						<Trash2 className="w-4 h-4" />
					</Button>
				</div>

				{/* Zoom controls */}
				<div className="flex items-center gap-0.5 mr-2">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={zoomOut}
						className="h-8 w-8 p-0"
						title="Zoom out"
					>
						<ZoomOut className="w-4 h-4" />
					</Button>
					<button
						type="button"
						onClick={resetZoom}
						className="text-xs px-1 min-w-[40px] text-center"
						title="Reset zoom"
					>
						{Math.round(scale * 100)}%
					</button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={zoomIn}
						className="h-8 w-8 p-0"
						title="Zoom in"
					>
						<ZoomIn className="w-4 h-4" />
					</Button>
				</div>

				{/* Upload and Save buttons */}
				<div className="flex items-center gap-0.5">
					<input
						ref={fileInputRef}
						type="file"
						accept="image/*"
						onChange={handleFileUpload}
						className="hidden"
					/>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={() => fileInputRef.current?.click()}
						className="h-8 px-2"
						title="Upload image"
					>
						<Upload className="w-4 h-4 mr-1" />
						<span className="text-xs">Upload</span>
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={saveAsImage}
						className="h-8 px-2"
						title="Download as PNG"
					>
						<Download className="w-4 h-4 mr-1" />
						<span className="text-xs">Export</span>
					</Button>
					{workspacePath && (
						<>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={saveToWorkspace}
								className="h-8 px-2"
								title="Save to workspace"
							>
								<Save className="w-4 h-4 mr-1" />
								<span className="text-xs">Save</span>
							</Button>
							{onSaveAndAddToChat && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={saveAndAddToChat}
									className="h-8 px-2"
									title="Save and add to chat"
								>
									<MessageSquarePlus className="w-4 h-4 mr-1" />
									<span className="text-xs">Add to Chat</span>
								</Button>
							)}
						</>
					)}
				</div>
			</div>

			{/* Canvas area - background matches Konva canvas to prevent flash */}
			<div
				ref={containerRef}
				className="flex-1 overflow-hidden relative"
				style={{ backgroundColor: canvasBackgroundColor }}
			>
				<Stage
					ref={stageRef}
					width={containerSize.width}
					height={containerSize.height}
					scaleX={scale}
					scaleY={scale}
					x={position.x}
					y={position.y}
					draggable={tool === "pan"}
					onDragEnd={(e) => {
						if (tool === "pan") {
							setPosition({ x: e.target.x(), y: e.target.y() });
						}
					}}
					onMouseDown={handleMouseDown}
					onMouseMove={handleMouseMove}
					onMouseUp={handleMouseUp}
					onTouchStart={handleMouseDown}
					onTouchMove={handleMouseMove}
					onTouchEnd={handleMouseUp}
					style={{ cursor: tool === "pan" ? "grab" : "crosshair" }}
				>
					{/* Background layer */}
					<Layer>
						{/* Canvas background - adapts to dark mode */}
						<Rect
							x={0}
							y={0}
							width={canvasWidth}
							height={canvasHeight}
							fill={canvasBackgroundColor}
						/>
						{/* Background image */}
						{backgroundImage && (
							<KonvaImage
								image={backgroundImage}
								x={0}
								y={0}
								width={backgroundSize.width}
								height={backgroundSize.height}
							/>
						)}
					</Layer>

					{/* Annotations layer */}
					<Layer>
						{annotations.map((annotation) => (
							<AnnotationShape
								key={annotation.id}
								annotation={annotation}
								isSelected={annotation.id === selectedId}
								onSelect={() => setSelectedId(annotation.id)}
								onChange={(newAttrs) =>
									updateAnnotation(annotation.id, newAttrs)
								}
								image={
									annotation.type === "image"
										? imageCache.get(annotation.id)
										: undefined
								}
							/>
						))}

						{/* Current drawing */}
						{currentAnnotation && (
							<AnnotationShape
								annotation={currentAnnotation}
								isSelected={false}
								onSelect={() => {}}
								onChange={() => {}}
							/>
						)}

						{/* Transformer for selected shape */}
						{selectedId && (
							<Transformer
								ref={transformerRef}
								boundBoxFunc={(oldBox, newBox) => {
									// Limit minimum size
									if (newBox.width < 5 || newBox.height < 5) {
										return oldBox;
									}
									return newBox;
								}}
							/>
						)}
					</Layer>
				</Stage>

				{/* Text input overlay */}
				{textInputPosition && (
					<div
						style={{
							position: "absolute",
							left: textInputPosition.x * scale + position.x,
							top: textInputPosition.y * scale + position.y,
						}}
					>
						<input
							ref={textInputRef}
							type="text"
							value={textInput}
							onChange={(e) => setTextInput(e.target.value)}
							onKeyDown={(e) => {
								// Stop propagation to prevent canvas shortcuts from firing
								e.stopPropagation();
								if (e.key === "Enter") {
									handleTextSubmit();
								}
								if (e.key === "Escape") {
									setTextInputPosition(null);
									setTextInput("");
								}
							}}
							onBlur={handleTextSubmit}
							className="min-w-[100px] h-7 text-sm z-50 px-2 border border-input bg-background"
							placeholder="Enter text..."
						/>
					</div>
				)}
			</div>

			{/* Status bar */}
			<div className="flex-shrink-0 flex items-center gap-2 px-2 py-1 border-t border-border bg-muted/30 text-xs text-muted-foreground">
				<span className="whitespace-nowrap">
					{annotations.length} annotation{annotations.length !== 1 ? "s" : ""}
				</span>
				<span className="whitespace-nowrap">
					{canvasWidth}x{canvasHeight}
				</span>
				<span className="hidden sm:inline opacity-70">
					Paste to add | V H P A R C T E
				</span>
			</div>
		</div>
	);
});
