"use client";

import { Button } from "@/components/ui/button";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import {
	Select,
	SelectContent,
	SelectGroup,
	SelectItem,
	SelectLabel,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { readFileMux, writeFileMux } from "@/lib/mux-files";
import { cn } from "@/lib/utils";
import type Konva from "konva";
import {
	ArrowRight,
	Bold,
	Circle,
	Download,
	Eraser,
	Hand,
	Highlighter,
	Italic,
	LayoutTemplate,
	MessageSquarePlus,
	Minus,
	MousePointer2,
	Paintbrush,
	Pencil,
	Redo2,
	Save,
	Square,
	Star,
	Trash2,
	Triangle,
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
	Star as KonvaStar,
	Layer,
	Line,
	Rect,
	RegularPolygon,
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
	| "line"
	| "arrow"
	| "rect"
	| "circle"
	| "triangle"
	| "star"
	| "text"
	| "highlighter"
	| "eraser";

// --- Font definitions ---
const FONT_FAMILIES = [
	{ name: "Impact", value: "Impact", category: "meme" as const },
	{
		name: "Comic Sans MS",
		value: "'Comic Sans MS', cursive",
		category: "meme" as const,
	},
	{ name: "Arial", value: "Arial, sans-serif", category: "sans" as const },
	{
		name: "Helvetica",
		value: "Helvetica, Arial, sans-serif",
		category: "sans" as const,
	},
	{ name: "Verdana", value: "Verdana, sans-serif", category: "sans" as const },
	{ name: "Georgia", value: "Georgia, serif", category: "serif" as const },
	{
		name: "Times New Roman",
		value: "'Times New Roman', serif",
		category: "serif" as const,
	},
	{
		name: "Courier New",
		value: "'Courier New', monospace",
		category: "mono" as const,
	},
] as const;

type FontCategory = "meme" | "sans" | "serif" | "mono";

const FONT_CATEGORY_LABELS: Record<FontCategory, string> = {
	meme: "Meme Fonts",
	sans: "Sans-Serif",
	serif: "Serif",
	mono: "Monospace",
};

// --- Preset colors ---
const PRESET_COLORS = [
	"#ff0000",
	"#ff6600",
	"#ffff00",
	"#00ff00",
	"#0000ff",
	"#ff00ff",
	"#00ffff",
	"#ffffff",
	"#000000",
	"#808080",
] as const;

// --- Meme template definitions ---
interface MemeTemplate {
	id: string;
	name: string;
	category: "classic" | "modern" | "reaction" | "layout";
	width: number;
	height: number;
	backgroundColor: string;
	elements: Omit<Annotation, "id">[];
	description?: string;
}

const MEME_TEMPLATES: MemeTemplate[] = [
	{
		id: "classic-top-bottom",
		name: "Classic Top/Bottom",
		category: "classic",
		description: "Bold Impact text on top and bottom",
		width: 600,
		height: 600,
		backgroundColor: "#1a1a2e",
		elements: [
			{
				type: "text",
				x: 300,
				y: 40,
				text: "TOP TEXT",
				fontSize: 48,
				fontFamily: "Impact",
				fontStyle: "normal",
				fill: "#ffffff",
				textStroke: "#000000",
				textStrokeWidth: 3,
				align: "center",
				width: 560,
			},
			{
				type: "text",
				x: 300,
				y: 520,
				text: "BOTTOM TEXT",
				fontSize: 48,
				fontFamily: "Impact",
				fontStyle: "normal",
				fill: "#ffffff",
				textStroke: "#000000",
				textStrokeWidth: 3,
				align: "center",
				width: 560,
			},
		],
	},
	{
		id: "drake-format",
		name: "Drake / Two Panel",
		category: "classic",
		description: "Two rows: nah on top, yes on bottom",
		width: 600,
		height: 600,
		backgroundColor: "#f5f5f5",
		elements: [
			{
				type: "line",
				x: 0,
				y: 300,
				points: [0, 0, 600, 0],
				stroke: "#333",
				strokeWidth: 3,
			},
			{
				type: "line",
				x: 300,
				y: 0,
				points: [0, 0, 0, 600],
				stroke: "#333",
				strokeWidth: 3,
			},
			{
				type: "text",
				x: 150,
				y: 130,
				text: "NAH",
				fontSize: 36,
				fontFamily: "Impact",
				fill: "#cc3333",
				textStroke: "#000000",
				textStrokeWidth: 1,
				align: "center",
				width: 260,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 450,
				y: 100,
				text: "Something boring",
				fontSize: 24,
				fontFamily: "Arial, sans-serif",
				fill: "#333",
				align: "center",
				width: 260,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 150,
				y: 430,
				text: "YES",
				fontSize: 36,
				fontFamily: "Impact",
				fill: "#33aa33",
				textStroke: "#000000",
				textStrokeWidth: 1,
				align: "center",
				width: 260,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 450,
				y: 400,
				text: "Something awesome",
				fontSize: 24,
				fontFamily: "Arial, sans-serif",
				fill: "#333",
				align: "center",
				width: 260,
				fontStyle: "normal",
			},
		],
	},
	{
		id: "expanding-brain",
		name: "Expanding Brain",
		category: "classic",
		description: "Four panels of escalating enlightenment",
		width: 600,
		height: 800,
		backgroundColor: "#ffffff",
		elements: [
			{
				type: "line",
				x: 0,
				y: 200,
				points: [0, 0, 600, 0],
				stroke: "#ccc",
				strokeWidth: 2,
			},
			{
				type: "line",
				x: 0,
				y: 400,
				points: [0, 0, 600, 0],
				stroke: "#ccc",
				strokeWidth: 2,
			},
			{
				type: "line",
				x: 0,
				y: 600,
				points: [0, 0, 600, 0],
				stroke: "#ccc",
				strokeWidth: 2,
			},
			{
				type: "text",
				x: 300,
				y: 80,
				text: "Normal idea",
				fontSize: 28,
				fontFamily: "Arial, sans-serif",
				fill: "#333",
				align: "center",
				width: 560,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 300,
				y: 280,
				text: "Better idea",
				fontSize: 28,
				fontFamily: "Arial, sans-serif",
				fill: "#333",
				align: "center",
				width: 560,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 300,
				y: 480,
				text: "Galaxy brain idea",
				fontSize: 28,
				fontFamily: "Arial, sans-serif",
				fill: "#333",
				align: "center",
				width: 560,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 300,
				y: 680,
				text: "TRANSCENDENT",
				fontSize: 32,
				fontFamily: "Impact",
				fill: "#ff6600",
				textStroke: "#000000",
				textStrokeWidth: 1,
				align: "center",
				width: 560,
				fontStyle: "normal",
			},
		],
	},
	{
		id: "motivational",
		name: "Motivational Poster",
		category: "modern",
		description: "Black border with serif caption",
		width: 600,
		height: 750,
		backgroundColor: "#000000",
		elements: [
			{
				type: "rect",
				x: 40,
				y: 40,
				width: 520,
				height: 520,
				stroke: "#444",
				strokeWidth: 2,
				fill: "#222",
			},
			{
				type: "text",
				x: 300,
				y: 600,
				text: "MOTIVATION",
				fontSize: 42,
				fontFamily: "Georgia, serif",
				fill: "#ffffff",
				align: "center",
				width: 520,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 300,
				y: 670,
				text: "If you can dream it, you can meme it",
				fontSize: 18,
				fontFamily: "Georgia, serif",
				fontStyle: "italic",
				fill: "#cccccc",
				align: "center",
				width: 480,
			},
		],
	},
	{
		id: "social-post",
		name: "Social Media Post",
		category: "modern",
		description: "Square format with clean text",
		width: 600,
		height: 600,
		backgroundColor: "#667eea",
		elements: [
			{
				type: "text",
				x: 300,
				y: 240,
				text: "Your bold\nstatement here",
				fontSize: 52,
				fontFamily: "Helvetica, Arial, sans-serif",
				fontStyle: "bold",
				fill: "#ffffff",
				align: "center",
				width: 500,
			},
			{
				type: "text",
				x: 300,
				y: 440,
				text: "Supporting text goes here",
				fontSize: 20,
				fontFamily: "Helvetica, Arial, sans-serif",
				fill: "#ffffffcc",
				align: "center",
				width: 460,
				fontStyle: "normal",
			},
		],
	},
	{
		id: "this-is-fine",
		name: "This Is Fine",
		category: "reaction",
		description: "Single panel with caption",
		width: 600,
		height: 400,
		backgroundColor: "#ffcc00",
		elements: [
			{
				type: "text",
				x: 300,
				y: 340,
				text: "THIS IS FINE",
				fontSize: 42,
				fontFamily: "Impact",
				fill: "#ffffff",
				textStroke: "#000000",
				textStrokeWidth: 3,
				align: "center",
				width: 560,
				fontStyle: "normal",
			},
		],
	},
	{
		id: "three-panel",
		name: "Three Panel",
		category: "reaction",
		description: "Three vertical panels side by side",
		width: 900,
		height: 600,
		backgroundColor: "#f0f0f0",
		elements: [
			{
				type: "line",
				x: 300,
				y: 0,
				points: [0, 0, 0, 600],
				stroke: "#ccc",
				strokeWidth: 2,
			},
			{
				type: "line",
				x: 600,
				y: 0,
				points: [0, 0, 0, 600],
				stroke: "#ccc",
				strokeWidth: 2,
			},
			{
				type: "text",
				x: 150,
				y: 530,
				text: "Panel 1",
				fontSize: 24,
				fontFamily: "Impact",
				fill: "#ffffff",
				textStroke: "#000000",
				textStrokeWidth: 2,
				align: "center",
				width: 260,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 450,
				y: 530,
				text: "Panel 2",
				fontSize: 24,
				fontFamily: "Impact",
				fill: "#ffffff",
				textStroke: "#000000",
				textStrokeWidth: 2,
				align: "center",
				width: 260,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 750,
				y: 530,
				text: "Panel 3",
				fontSize: 24,
				fontFamily: "Impact",
				fill: "#ffffff",
				textStroke: "#000000",
				textStrokeWidth: 2,
				align: "center",
				width: 260,
				fontStyle: "normal",
			},
		],
	},
	{
		id: "comparison",
		name: "Left vs Right",
		category: "layout",
		description: "Side-by-side comparison",
		width: 800,
		height: 500,
		backgroundColor: "#ffffff",
		elements: [
			{
				type: "rect",
				x: 0,
				y: 0,
				width: 400,
				height: 500,
				stroke: "transparent",
				strokeWidth: 0,
				fill: "#1a1a2e",
			},
			{
				type: "text",
				x: 200,
				y: 60,
				text: "Option A",
				fontSize: 36,
				fontFamily: "Helvetica, Arial, sans-serif",
				fontStyle: "bold",
				fill: "#ffffff",
				align: "center",
				width: 340,
			},
			{
				type: "text",
				x: 600,
				y: 60,
				text: "Option B",
				fontSize: 36,
				fontFamily: "Helvetica, Arial, sans-serif",
				fontStyle: "bold",
				fill: "#1a1a2e",
				align: "center",
				width: 340,
			},
			{
				type: "text",
				x: 400,
				y: 220,
				text: "VS",
				fontSize: 32,
				fontFamily: "Impact",
				fill: "#ff6600",
				align: "center",
				width: 80,
				fontStyle: "normal",
			},
		],
	},
	{
		id: "announcement",
		name: "Announcement Banner",
		category: "layout",
		description: "Wide banner format",
		width: 900,
		height: 300,
		backgroundColor: "#16213e",
		elements: [
			{
				type: "rect",
				x: 0,
				y: 0,
				width: 6,
				height: 300,
				stroke: "transparent",
				strokeWidth: 0,
				fill: "#3ba77c",
			},
			{
				type: "text",
				x: 450,
				y: 100,
				text: "BIG ANNOUNCEMENT",
				fontSize: 52,
				fontFamily: "Impact",
				fill: "#ffffff",
				align: "center",
				width: 800,
				fontStyle: "normal",
			},
			{
				type: "text",
				x: 450,
				y: 200,
				text: "Details about the thing",
				fontSize: 20,
				fontFamily: "Arial, sans-serif",
				fill: "#8892b0",
				align: "center",
				width: 700,
				fontStyle: "normal",
			},
		],
	},
	{
		id: "blank-canvas",
		name: "Blank Canvas",
		category: "layout",
		description: "Empty canvas to start from scratch",
		width: 800,
		height: 600,
		backgroundColor: "#ffffff",
		elements: [],
	},
];

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

interface TriangleAnnotation extends AnnotationBase {
	type: "triangle";
	radius: number;
	stroke: string;
	strokeWidth: number;
	fill?: string;
}

interface StarAnnotation extends AnnotationBase {
	type: "star";
	innerRadius: number;
	outerRadius: number;
	numPoints: number;
	stroke: string;
	strokeWidth: number;
	fill?: string;
}

interface TextAnnotation extends AnnotationBase {
	type: "text";
	text: string;
	fontSize: number;
	fill: string;
	fontFamily?: string;
	fontStyle?: string;
	textStroke?: string;
	textStrokeWidth?: number;
	align?: "left" | "center" | "right";
	width?: number;
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
	| TriangleAnnotation
	| StarAnnotation
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
	fillColor: string;
	strokeWidth: number;
	fontSize: number;
	fontFamily: string;
	fontStyle: string;
	textStroke: string;
	textStrokeWidth: number;
	backgroundImageSrc: string | null;
	backgroundSize: { width: number; height: number };
	canvasBackgroundOverride: string | null;
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
			className="h-[26px] w-[26px] p-0"
			title={title}
		>
			<Icon className="w-[14px] h-[14px]" />
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
		case "triangle":
			return (
				<RegularPolygon
					ref={shapeRef as React.RefObject<Konva.RegularPolygon>}
					x={annotation.x}
					y={annotation.y}
					sides={3}
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
		case "star":
			return (
				<KonvaStar
					ref={shapeRef as React.RefObject<Konva.Star>}
					x={annotation.x}
					y={annotation.y}
					numPoints={annotation.numPoints}
					innerRadius={annotation.innerRadius}
					outerRadius={annotation.outerRadius}
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
					width={annotation.width}
					text={annotation.text}
					fontSize={annotation.fontSize}
					fontFamily={annotation.fontFamily ?? "Arial, sans-serif"}
					fontStyle={annotation.fontStyle ?? "normal"}
					fill={annotation.fill}
					stroke={
						annotation.textStroke && annotation.textStroke !== "transparent"
							? annotation.textStroke
							: undefined
					}
					strokeWidth={annotation.textStrokeWidth ?? 0}
					align={annotation.align ?? "left"}
					lineHeight={1.2}
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
	const [fillColor, setFillColor] = useState(
		cached?.fillColor ?? "transparent",
	);
	const [strokeWidth, setStrokeWidth] = useState(cached?.strokeWidth ?? 3);
	const [fontSize, setFontSize] = useState(cached?.fontSize ?? 48);
	const [fontFamily, setFontFamily] = useState(cached?.fontFamily ?? "Impact");
	const [fontStyle, setFontStyle] = useState(cached?.fontStyle ?? "normal");
	const [textStroke, setTextStroke] = useState(cached?.textStroke ?? "#000000");
	const [textStrokeWidth, setTextStrokeWidth] = useState(
		cached?.textStrokeWidth ?? 2,
	);
	const [canvasBackgroundOverride, setCanvasBackgroundOverride] = useState<
		string | null
	>(cached?.canvasBackgroundOverride ?? null);
	const [showTemplates, setShowTemplates] = useState(false);
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
			fillColor,
			strokeWidth,
			fontSize,
			fontFamily,
			fontStyle,
			textStroke,
			textStrokeWidth,
			backgroundImageSrc,
			backgroundSize,
			canvasBackgroundOverride,
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
		fillColor,
		strokeWidth,
		fontSize,
		fontFamily,
		fontStyle,
		textStroke,
		textStrokeWidth,
		backgroundImageSrc,
		backgroundSize,
		canvasBackgroundOverride,
		initialImagePath,
	]);

	// Drawing state
	const [isPanning, setIsPanning] = useState(false);
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
	const textInputRef = useRef<HTMLTextAreaElement>(null);
	const backgroundUrlRef = useRef<string | null>(null);

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

	const canvasBackgroundColor =
		canvasBackgroundOverride ?? (isDarkMode ? "#2d312f" : "#ffffff");

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
		if (!initialImagePath || !workspacePath) return;
		let cancelled = false;

		const load = async () => {
			try {
				const result = await readFileMux(workspacePath, initialImagePath);
				if (cancelled) return;
				const blob = new Blob([result.data]);
				const url = URL.createObjectURL(blob);
				if (backgroundUrlRef.current?.startsWith("blob:")) {
					URL.revokeObjectURL(backgroundUrlRef.current);
				}
				backgroundUrlRef.current = url;

				const img = new Image();
				img.onload = () => {
					if (cancelled) return;
					setBackgroundImage(img);
					setBackgroundImageSrc(url);
					setBackgroundSize({ width: img.width, height: img.height });
				};
				img.src = url;
			} catch (err) {
				console.error("Failed to load canvas image:", err);
			}
		};

		void load();

		return () => {
			cancelled = true;
		};
	}, [initialImagePath, workspacePath]);

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
		setCanvasBackgroundOverride(null);
		clearCanvasCache();
	}, [addToHistory]);

	// Load a meme template
	const loadTemplate = useCallback(
		(template: MemeTemplate) => {
			const newAnnotations: Annotation[] = template.elements.map(
				(el) =>
					({
						...el,
						id: generateId(),
					}) as Annotation,
			);
			setAnnotations(newAnnotations);
			addToHistory(newAnnotations);
			setSelectedId(null);
			setBackgroundImage(null);
			setBackgroundImageSrc(null);
			setCanvasBackgroundOverride(template.backgroundColor);
			setBackgroundSize({ width: template.width, height: template.height });
			setImageCache(new Map());
			setShowTemplates(false);
			setScale(1);
			setPosition({ x: 0, y: 0 });
		},
		[addToHistory],
	);

	// Zoom controls
	const zoomIn = useCallback(() => {
		setScale((prev) => Math.min(prev * 1.2, 10));
	}, []);

	const zoomOut = useCallback(() => {
		setScale((prev) => Math.max(prev / 1.2, 0.01));
	}, []);

	const resetZoom = useCallback(() => {
		setScale(1);
		setPosition({ x: 0, y: 0 });
	}, []);

	const fitToView = useCallback(() => {
		if (containerSize.width <= 0 || containerSize.height <= 0) return;
		const cw = backgroundImage ? backgroundSize.width : 800;
		const ch = backgroundImage ? backgroundSize.height : 600;
		if (cw <= 0 || ch <= 0) return;
		const padding = 40;
		const availW = containerSize.width - padding * 2;
		const availH = containerSize.height - padding * 2;
		if (availW <= 0 || availH <= 0) return;
		const fitScale = Math.min(availW / cw, availH / ch, 1);
		const offsetX = (containerSize.width - cw * fitScale) / 2;
		const offsetY = (containerSize.height - ch * fitScale) / 2;
		setScale(fitScale);
		setPosition({ x: offsetX, y: offsetY });
	}, [backgroundImage, backgroundSize, containerSize]);

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
				case "line":
					setCurrentAnnotation({
						id,
						type: "line",
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
						fill: fillColor === "transparent" ? undefined : fillColor,
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
						fill: fillColor === "transparent" ? undefined : fillColor,
					});
					break;
				case "triangle":
					setCurrentAnnotation({
						id,
						type: "triangle",
						x: adjustedPos.x,
						y: adjustedPos.y,
						radius: 0,
						stroke: color,
						strokeWidth,
						fill: fillColor === "transparent" ? undefined : fillColor,
					});
					break;
				case "star":
					setCurrentAnnotation({
						id,
						type: "star",
						x: adjustedPos.x,
						y: adjustedPos.y,
						innerRadius: 0,
						outerRadius: 0,
						numPoints: 5,
						stroke: color,
						strokeWidth,
						fill: fillColor === "transparent" ? undefined : fillColor,
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
		[
			tool,
			color,
			fillColor,
			strokeWidth,
			scale,
			position,
			annotations,
			deleteAnnotation,
		],
	);

	// biome-ignore lint/correctness/useExhaustiveDependencies: tool is stable ref
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
					// If it has exactly 4 points, it's a straight line tool (not pencil)
					if (tool === "line" && currentAnnotation.points.length === 4) {
						setCurrentAnnotation({
							...currentAnnotation,
							points: [
								currentAnnotation.points[0],
								currentAnnotation.points[1],
								adjustedPos.x,
								adjustedPos.y,
							],
						});
					} else {
						setCurrentAnnotation({
							...currentAnnotation,
							points: [
								...currentAnnotation.points,
								adjustedPos.x,
								adjustedPos.y,
							],
						});
					}
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
				case "triangle": {
					const tdx = adjustedPos.x - currentAnnotation.x;
					const tdy = adjustedPos.y - currentAnnotation.y;
					setCurrentAnnotation({
						...currentAnnotation,
						radius: Math.sqrt(tdx * tdx + tdy * tdy),
					});
					break;
				}
				case "star": {
					const sdx = adjustedPos.x - currentAnnotation.x;
					const sdy = adjustedPos.y - currentAnnotation.y;
					const outerRadius = Math.sqrt(sdx * sdx + sdy * sdy);
					setCurrentAnnotation({
						...currentAnnotation,
						outerRadius,
						innerRadius: outerRadius * 0.4,
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
			fontFamily,
			fontStyle,
			fill: color,
			textStroke: textStrokeWidth > 0 ? textStroke : undefined,
			textStrokeWidth: textStrokeWidth > 0 ? textStrokeWidth : undefined,
			align: "left",
			width: 300,
		};
		addAnnotation(annotation);
		setTextInputPosition(null);
		setTextInput("");
	}, [
		textInputPosition,
		textInput,
		color,
		fontSize,
		fontFamily,
		fontStyle,
		textStroke,
		textStrokeWidth,
		addAnnotation,
	]);

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
		if (!stageRef.current || !workspacePath) return null;

		const dataUrl = stageRef.current.toDataURL({ pixelRatio: 2 });

		const response = await fetch(dataUrl);
		const buffer = await response.arrayBuffer();
		const filename = `annotated-${Date.now()}.png`;

		try {
			await writeFileMux(workspacePath, filename, buffer, true);
			console.log("Saved to workspace:", filename);
			return filename;
		} catch (err) {
			console.error("Failed to save:", err);
			return null;
		}
	}, [workspacePath]);

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
					case "l":
						setTool("line");
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

	// Container size - start at 0 so Stage is not rendered until measured
	const [containerSize, setContainerSize] = useState({
		width: 0,
		height: 0,
	});

	useEffect(() => {
		const el = containerRef.current;
		if (!el) return;

		const updateSize = () => {
			setContainerSize({
				width: el.offsetWidth,
				height: el.offsetHeight,
			});
		};

		updateSize();

		if (typeof ResizeObserver !== "undefined") {
			const ro = new ResizeObserver(updateSize);
			ro.observe(el);
			return () => ro.disconnect();
		}
		window.addEventListener("resize", updateSize);
		return () => window.removeEventListener("resize", updateSize);
	}, []);

	// Auto-fit when a new background image is loaded that exceeds container
	const prevBgRef = useRef<HTMLImageElement | null>(null);
	useEffect(() => {
		if (backgroundImage && backgroundImage !== prevBgRef.current) {
			prevBgRef.current = backgroundImage;
			// Only auto-fit if the image is larger than the container
			if (
				backgroundSize.width > containerSize.width ||
				backgroundSize.height > containerSize.height
			) {
				fitToView();
			}
		}
	}, [backgroundImage, backgroundSize, containerSize, fitToView]);

	// Calculate canvas size (use container size or background size, whichever is available)
	const canvasWidth = backgroundImage
		? backgroundSize.width
		: containerSize.width || 800;
	const canvasHeight = backgroundImage
		? backgroundSize.height
		: containerSize.height || 600;

	return (
		<div
			className={cn("flex flex-col h-full overflow-hidden", className)}
			data-spotlight="canvas"
		>
			{/* Toolbar - Variant E: two dense rows */}
			<div className="flex-shrink-0 border-b border-border bg-muted/30">
				{/* Row 1: All drawing tools in one line with separator groups */}
				<div className="flex items-center gap-px p-1">
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
					<div className="w-px h-[18px] bg-border mx-[3px]" />
					<ToolButton
						tool="pencil"
						currentTool={tool}
						onSelect={setTool}
						icon={Pencil}
						title="Pencil (P)"
					/>
					<ToolButton
						tool="line"
						currentTool={tool}
						onSelect={setTool}
						icon={Minus}
						title="Line (L)"
					/>
					<ToolButton
						tool="arrow"
						currentTool={tool}
						onSelect={setTool}
						icon={ArrowRight}
						title="Arrow (A)"
					/>
					<div className="w-px h-[18px] bg-border mx-[3px]" />
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
						tool="triangle"
						currentTool={tool}
						onSelect={setTool}
						icon={Triangle}
						title="Triangle"
					/>
					<ToolButton
						tool="star"
						currentTool={tool}
						onSelect={setTool}
						icon={Star}
						title="Star"
					/>
					<div className="w-px h-[18px] bg-border mx-[3px]" />
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

				{/* Row 2: Colors + stroke | undo/redo | zoom | actions */}
				<div className="flex items-center gap-[3px] px-1.5 pb-1.5 border-t border-white/[0.04]">
					{/* Stroke color */}
					<Popover>
						<PopoverTrigger asChild>
							<button
								type="button"
								className="w-[18px] h-[18px] rounded-[3px] border-2 border-[#555] cursor-pointer flex-shrink-0"
								style={{ backgroundColor: color }}
								title="Stroke color"
							/>
						</PopoverTrigger>
						<PopoverContent className="w-auto p-2" align="start">
							<div className="grid grid-cols-5 gap-1 mb-2">
								{PRESET_COLORS.map((c) => (
									<button
										type="button"
										key={c}
										onClick={() => setColor(c)}
										className={cn(
											"w-6 h-6 rounded border",
											color === c
												? "ring-2 ring-primary ring-offset-1"
												: "border-border",
										)}
										style={{ backgroundColor: c }}
									/>
								))}
							</div>
							<input
								type="color"
								value={color}
								onChange={(e) => setColor(e.target.value)}
								className="w-full h-7 cursor-pointer"
							/>
						</PopoverContent>
					</Popover>
					{/* Fill color */}
					<Popover>
						<PopoverTrigger asChild>
							<button
								type="button"
								className="w-[18px] h-[18px] rounded-[3px] border-2 border-[#555] cursor-pointer flex-shrink-0 flex items-center justify-center"
								style={{
									backgroundColor:
										fillColor === "transparent" ? "transparent" : fillColor,
								}}
								title="Fill color"
							>
								{fillColor === "transparent" && (
									// biome-ignore lint/a11y/noSvgWithoutTitle: decorative SVG -- in expression context
									<svg width="10" height="10" viewBox="0 0 10 10">
										<line
											x1="0"
											y1="10"
											x2="10"
											y2="0"
											stroke="#ff0000"
											strokeWidth="2"
										/>
									</svg>
								)}
							</button>
						</PopoverTrigger>
						<PopoverContent className="w-auto p-2" align="start">
							<div className="grid grid-cols-5 gap-1 mb-2">
								<button
									type="button"
									onClick={() => setFillColor("transparent")}
									className={cn(
										"w-6 h-6 rounded border flex items-center justify-center",
										fillColor === "transparent"
											? "ring-2 ring-primary ring-offset-1"
											: "border-border",
									)}
									title="No fill"
								>
									{/* biome-ignore lint/a11y/noSvgWithoutTitle: decorative SVG */}
									<svg width="10" height="10" viewBox="0 0 10 10">
										<line
											x1="0"
											y1="10"
											x2="10"
											y2="0"
											stroke="#ff0000"
											strokeWidth="2"
										/>
									</svg>
								</button>
								{PRESET_COLORS.slice(0, 9).map((c) => (
									<button
										type="button"
										key={c}
										onClick={() => setFillColor(c)}
										className={cn(
											"w-6 h-6 rounded border",
											fillColor === c
												? "ring-2 ring-primary ring-offset-1"
												: "border-border",
										)}
										style={{ backgroundColor: c }}
									/>
								))}
							</div>
							<input
								type="color"
								value={fillColor === "transparent" ? "#ffffff" : fillColor}
								onChange={(e) => setFillColor(e.target.value)}
								className="w-full h-7 cursor-pointer"
							/>
						</PopoverContent>
					</Popover>
					<select
						value={strokeWidth}
						onChange={(e) => setStrokeWidth(Number(e.target.value))}
						className="h-[22px] text-[10px] bg-background border border-border rounded-[3px] px-1"
						title="Stroke width"
					>
						<option value={1}>1px</option>
						<option value={2}>2px</option>
						<option value={3}>3px</option>
						<option value={5}>5px</option>
						<option value={8}>8px</option>
						<option value={12}>12px</option>
					</select>
					<div className="w-px h-4 bg-border" />
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={undo}
						disabled={historyIndex <= 0}
						className="h-[22px] w-[22px] p-0"
						title="Undo (Ctrl+Z)"
					>
						<Undo2 className="w-3 h-3" />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={redo}
						disabled={historyIndex >= history.length - 1}
						className="h-[22px] w-[22px] p-0"
						title="Redo (Ctrl+Y)"
					>
						<Redo2 className="w-3 h-3" />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={clearAll}
						className="h-[22px] w-[22px] p-0"
						title="Clear all"
					>
						<Trash2 className="w-3 h-3" />
					</Button>
					<div className="w-px h-4 bg-border" />
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={zoomOut}
						className="h-[22px] w-[22px] p-0"
						title="Zoom out"
					>
						<ZoomOut className="w-3 h-3" />
					</Button>
					<button
						type="button"
						onClick={resetZoom}
						className="text-[10px] min-w-[30px] text-center text-muted-foreground tabular-nums"
						title="Reset zoom"
					>
						{Math.round(scale * 100)}%
					</button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={zoomIn}
						className="h-[22px] w-[22px] p-0"
						title="Zoom in"
					>
						<ZoomIn className="w-3 h-3" />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={fitToView}
						className="h-[22px] px-1.5 text-[10px]"
						title="Fit to view"
					>
						Fit
					</Button>
					<div className="flex-1" />
					<Popover open={showTemplates} onOpenChange={setShowTemplates}>
						<PopoverTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="h-[22px] w-[22px] p-0"
								title="Templates"
							>
								<LayoutTemplate className="w-3 h-3" />
							</Button>
						</PopoverTrigger>
						<PopoverContent className="w-72 p-0" align="end">
							<div className="px-3 py-2 border-b">
								<h4 className="text-sm font-medium">Meme Templates</h4>
							</div>
							<div className="max-h-72 overflow-y-auto p-1.5 grid gap-0.5">
								{MEME_TEMPLATES.map((tmpl) => (
									<button
										type="button"
										key={tmpl.id}
										onClick={() => loadTemplate(tmpl)}
										className="text-left rounded-md border px-2 py-1.5 hover:bg-muted/50 transition-colors"
									>
										<div className="flex items-center gap-2">
											<div
												className="w-8 h-6 rounded border flex-shrink-0"
												style={{ backgroundColor: tmpl.backgroundColor }}
											/>
											<div className="min-w-0 flex-1">
												<div className="text-xs font-medium truncate">
													{tmpl.name}
												</div>
												<div className="text-[10px] text-muted-foreground truncate">
													{tmpl.description}
												</div>
											</div>
											<span className="text-[9px] px-1 py-0.5 rounded bg-muted text-muted-foreground flex-shrink-0">
												{tmpl.category}
											</span>
										</div>
									</button>
								))}
							</div>
						</PopoverContent>
					</Popover>
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
						className="h-[22px] w-[22px] p-0"
						title="Upload image"
					>
						<Upload className="w-3 h-3" />
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={saveAsImage}
						className="h-[22px] w-[22px] p-0"
						title="Export as PNG"
					>
						<Download className="w-3 h-3" />
					</Button>
					{workspacePath && (
						<>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={saveToWorkspace}
								className="h-[22px] w-[22px] p-0"
								title="Save to workspace"
							>
								<Save className="w-3 h-3" />
							</Button>
							{onSaveAndAddToChat && (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={saveAndAddToChat}
									className="h-[22px] w-[22px] p-0"
									title="Save & add to chat"
								>
									<MessageSquarePlus className="w-3 h-3" />
								</Button>
							)}
						</>
					)}
				</div>

				{/* Row 3 (conditional): Text controls */}
				{(tool === "text" ||
					(selectedId &&
						annotations.find((a) => a.id === selectedId)?.type === "text")) && (
					<div className="flex items-center gap-[3px] px-1.5 pb-1.5 border-t border-white/[0.04]">
						<Select value={fontFamily} onValueChange={setFontFamily}>
							<SelectTrigger className="h-[22px] w-24 text-[10px]">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{(["meme", "sans", "serif", "mono"] as FontCategory[]).map(
									(cat) => {
										const fonts = FONT_FAMILIES.filter(
											(f) => f.category === cat,
										);
										return (
											<SelectGroup key={cat}>
												<SelectLabel className="text-xs">
													{FONT_CATEGORY_LABELS[cat]}
												</SelectLabel>
												{fonts.map((f) => (
													<SelectItem key={f.value} value={f.value}>
														<span style={{ fontFamily: f.value }}>
															{f.name}
														</span>
													</SelectItem>
												))}
											</SelectGroup>
										);
									},
								)}
							</SelectContent>
						</Select>
						<select
							value={fontSize}
							onChange={(e) => setFontSize(Number(e.target.value))}
							className="h-[22px] text-[10px] bg-background border border-border rounded-[3px] px-1"
							title="Font size"
						>
							<option value={16}>16</option>
							<option value={20}>20</option>
							<option value={24}>24</option>
							<option value={32}>32</option>
							<option value={36}>36</option>
							<option value={48}>48</option>
							<option value={64}>64</option>
							<option value={72}>72</option>
							<option value={96}>96</option>
						</select>
						<Button
							type="button"
							variant={fontStyle.includes("bold") ? "default" : "ghost"}
							size="sm"
							className="h-[22px] w-[22px] p-0"
							title="Bold"
							onClick={() => {
								const b = fontStyle.includes("bold");
								const i = fontStyle.includes("italic");
								if (b && i) setFontStyle("italic");
								else if (b) setFontStyle("normal");
								else if (i) setFontStyle("bold italic");
								else setFontStyle("bold");
							}}
						>
							<Bold className="w-3 h-3" />
						</Button>
						<Button
							type="button"
							variant={fontStyle.includes("italic") ? "default" : "ghost"}
							size="sm"
							className="h-[22px] w-[22px] p-0"
							title="Italic"
							onClick={() => {
								const b = fontStyle.includes("bold");
								const i = fontStyle.includes("italic");
								if (i && b) setFontStyle("bold");
								else if (i) setFontStyle("normal");
								else if (b) setFontStyle("bold italic");
								else setFontStyle("italic");
							}}
						>
							<Italic className="w-3 h-3" />
						</Button>
						<div className="w-px h-4 bg-border" />
						<input
							type="color"
							value={textStroke}
							onChange={(e) => setTextStroke(e.target.value)}
							className="w-[18px] h-[18px] rounded-[3px] cursor-pointer border-0 p-0"
							title="Text outline color"
						/>
						<select
							value={textStrokeWidth}
							onChange={(e) => setTextStrokeWidth(Number(e.target.value))}
							className="h-[22px] text-[10px] bg-background border border-border rounded-[3px] px-1 w-9"
							title="Outline width"
						>
							<option value={0}>0</option>
							<option value={1}>1</option>
							<option value={2}>2</option>
							<option value={3}>3</option>
							<option value={4}>4</option>
							<option value={5}>5</option>
						</select>
					</div>
				)}
			</div>

			{/* Canvas area - background matches Konva canvas to prevent flash */}
			<div
				ref={containerRef}
				className="flex-1 overflow-hidden relative"
				style={{ backgroundColor: canvasBackgroundColor }}
			>
				{containerSize.width > 0 && containerSize.height > 0 && <Stage
					ref={stageRef}
					width={containerSize.width}
					height={containerSize.height}
					scaleX={scale}
					scaleY={scale}
					x={position.x}
					y={position.y}
					draggable={tool === "pan"}
					onDragStart={() => {
						if (tool === "pan") setIsPanning(true);
					}}
					onDragEnd={(e) => {
						if (tool === "pan") {
							setIsPanning(false);
							setPosition({ x: e.target.x(), y: e.target.y() });
						}
					}}
					onMouseDown={handleMouseDown}
					onMouseMove={handleMouseMove}
					onMouseUp={handleMouseUp}
					onTouchStart={handleMouseDown}
					onTouchMove={handleMouseMove}
					onTouchEnd={handleMouseUp}
					style={{
						cursor:
							tool === "pan"
								? isPanning
									? "grabbing"
									: "grab"
								: tool === "select"
									? "default"
									: tool === "eraser"
										? "not-allowed"
										: "crosshair",
					}}
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
				</Stage>}

				{/* Floating delete button for selected item (mobile-friendly) */}
				{selectedId &&
					!textInputPosition &&
					(() => {
						const ann = annotations.find((a) => a.id === selectedId);
						if (!ann) return null;
						// Compute screen position: annotation pos * scale + pan offset
						const screenX = ann.x * scale + position.x;
						const screenY = ann.y * scale + position.y;
						return (
							<button
								type="button"
								onClick={() => {
									deleteAnnotation(selectedId);
									setSelectedId(null);
								}}
								className="absolute z-40 flex items-center justify-center w-7 h-7 rounded-full bg-destructive text-destructive-foreground shadow-lg hover:bg-destructive/90 transition-colors"
								style={{
									left: screenX - 14,
									top: screenY - 32,
								}}
								title="Delete selected"
							>
								<Trash2 className="w-3.5 h-3.5" />
							</button>
						);
					})()}

				{/* Direct text input overlay - styled to match canvas rendering */}
				{textInputPosition && (
					<textarea
						ref={textInputRef}
						value={textInput}
						onChange={(e) => {
							setTextInput(e.target.value);
							// Auto-resize height to fit content
							const el = e.target;
							el.style.height = "auto";
							el.style.height = `${el.scrollHeight}px`;
						}}
						onKeyDown={(e) => {
							e.stopPropagation();
							if (e.key === "Enter" && !e.shiftKey) {
								e.preventDefault();
								handleTextSubmit();
							}
							if (e.key === "Escape") {
								setTextInputPosition(null);
								setTextInput("");
							}
						}}
						onBlur={handleTextSubmit}
						style={{
							position: "absolute",
							left: textInputPosition.x * scale + position.x,
							top: textInputPosition.y * scale + position.y,
							fontSize: `${fontSize * scale}px`,
							fontFamily,
							fontWeight: fontStyle.includes("bold") ? "bold" : "normal",
							fontStyle: fontStyle.includes("italic") ? "italic" : "normal",
							color,
							WebkitTextStroke:
								textStrokeWidth > 0
									? `${textStrokeWidth * scale}px ${textStroke}`
									: undefined,
							paintOrder: textStrokeWidth > 0 ? "stroke fill" : undefined,
							lineHeight: 1.2,
							minWidth: `${Math.max(60, 200 * scale)}px`,
							maxWidth: `${300 * scale}px`,
							minHeight: `${fontSize * scale * 1.2}px`,
							padding: "0",
							margin: "0",
							border: "1px dashed rgba(128,128,128,0.5)",
							outline: "none",
							background: "transparent",
							resize: "none",
							overflow: "hidden",
							zIndex: 50,
							caretColor: color,
						}}
						placeholder="Type here..."
					/>
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
					Paste to add | V H P L A R C T E
				</span>
			</div>
		</div>
	);
});
