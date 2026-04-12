"use client";

import { cn } from "@/lib/utils";
import { Check, Code, Copy, Minus, Plus, RotateCcw } from "lucide-react";
import { useTheme } from "next-themes";
import {
	Children,
	createContext,
	memo,
	useCallback,
	useContext,
	useEffect,
	useRef,
	useState,
} from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import {
	oneDark,
	oneLight,
} from "react-syntax-highlighter/dist/esm/styles/prism";
import remarkGfm from "remark-gfm";

interface MarkdownRendererProps {
	content: string;
	className?: string;
	enableMermaid?: boolean;
}

const MermaidEnabledContext = createContext(true);

const CopyButton = memo(function CopyButton({
	text,
	className,
}: { text: string; className?: string }) {
	const [copied, setCopied] = useState(false);

	const handleCopy = useCallback(async () => {
		try {
			if (navigator.clipboard?.writeText) {
				await navigator.clipboard.writeText(text);
			} else {
				// Fallback for older browsers or non-HTTPS contexts
				const textArea = document.createElement("textarea");
				textArea.value = text;
				textArea.style.position = "fixed";
				textArea.style.left = "-9999px";
				document.body.appendChild(textArea);
				textArea.select();
				document.execCommand("copy");
				document.body.removeChild(textArea);
			}
			setCopied(true);
			setTimeout(() => setCopied(false), 2000);
		} catch {
			// Silently fail if copy doesn't work
		}
	}, [text]);

	return (
		<button
			type="button"
			onClick={handleCopy}
			className={cn("p-1.5 text-muted-foreground", className)}
			title="Copy to clipboard"
		>
			{copied ? (
				<Check className="w-4 h-4 text-primary" />
			) : (
				<Copy className="w-4 h-4" />
			)}
		</button>
	);
});

function normalizeCodeContent(children: React.ReactNode): string {
	return Children.toArray(children)
		.map((child) => {
			if (typeof child === "string" || typeof child === "number") {
				return String(child);
			}
			return "";
		})
		.join("")
		.replace(/\n$/, "");
}

const MermaidCodeBlock = memo(function MermaidCodeBlock({
	codeString,
}: {
	codeString: string;
}) {
	const { resolvedTheme } = useTheme();
	const isDarkMode = resolvedTheme === "dark";
	const [viewMode, setViewMode] = useState<"diagram" | "code">("diagram");
	const [svg, setSvg] = useState<string | null>(null);
	const svgContainerRef = useRef<HTMLDivElement | null>(null);
	const mermaidSource = codeString.trim();
	const [error, setError] = useState<string | null>(null);
	const [isRendering, setIsRendering] = useState(true);
	const [isDiagramValid, setIsDiagramValid] = useState(true);
	const [zoomLevel, setZoomLevel] = useState(1);
	const ZOOM_MIN = 0.5;
	const ZOOM_MAX = 3;
	const ZOOM_STEP = 0.25;

	// useeffect-guardrail: allow - async mermaid rendering for content/theme changes
	useEffect(() => {
		let cancelled = false;

		const renderDiagram = async () => {
			setIsRendering(true);
			setError(null);
			setSvg(null);
			setZoomLevel(1);
			setIsDiagramValid(true);

			try {
				const mermaidModule = await import("mermaid");
				const mermaid = mermaidModule.default;
				mermaid.initialize({
					startOnLoad: false,
					securityLevel: "strict",
					theme: isDarkMode ? "dark" : "default",
					flowchart: {
						htmlLabels: false,
					},
				});
				const renderId = `mermaid-${Math.random().toString(36).slice(2, 10)}`;
				if (!mermaidSource) {
					throw new Error("Mermaid block is empty");
				}
				await mermaid.parse(mermaidSource);
				const { svg: renderedSvg } = await mermaid.render(
					renderId,
					mermaidSource,
				);

				if (cancelled) {
					return;
				}

				setSvg(renderedSvg);
				setIsDiagramValid(true);
				setIsRendering(false);
			} catch (renderError) {
				if (cancelled) {
					return;
				}
				const message =
					renderError instanceof Error
						? renderError.message
						: "Failed to render Mermaid diagram";
				setError(message);
				setIsDiagramValid(false);
				setViewMode("code");
				setIsRendering(false);
			}
		};

		void renderDiagram();

		return () => {
			cancelled = true;
		};
	}, [isDarkMode, mermaidSource]);

	// useeffect-guardrail: allow - inject rendered Mermaid SVG inline to avoid mobile img/blob/data URL failures
	useEffect(() => {
		if (viewMode !== "diagram") {
			return;
		}

		const container = svgContainerRef.current;
		if (!container) {
			return;
		}

		container.replaceChildren();
		if (!svg) {
			return;
		}

		container.insertAdjacentHTML("afterbegin", svg);
		const svgElement = container.querySelector("svg");
		if (!svgElement) {
			setError("Browser failed to mount Mermaid SVG output");
			setIsDiagramValid(false);
			setViewMode("code");
			return;
		}

		const viewBox = svgElement.getAttribute("viewBox");
		if (viewBox) {
			const values = viewBox
				.trim()
				.split(/\s+/)
				.map((part) => Number.parseFloat(part));
			if (
				values.length === 4 &&
				Number.isFinite(values[2]) &&
				Number.isFinite(values[3]) &&
				values[2] > 0 &&
				values[3] > 0
			) {
				svgElement.style.aspectRatio = `${values[2]} / ${values[3]}`;
			}
		}

		svgElement.removeAttribute("width");
		svgElement.removeAttribute("height");
		svgElement.setAttribute("width", "100%");
		svgElement.style.width = "100%";
		svgElement.style.maxWidth = "100%";
		svgElement.style.height = "auto";
		svgElement.style.display = "block";

		return () => {
			container.replaceChildren();
		};
	}, [svg, viewMode]);

	return (
		<div
			className="relative group my-3 overflow-hidden rounded-sm"
			style={{ borderColor: "var(--code-border)", borderWidth: "1px" }}
		>
			<div
				className="flex items-center justify-between px-3 py-1.5"
				style={{
					backgroundColor: "var(--code-bg)",
					borderBottomColor: "var(--code-border)",
					borderBottomWidth: "1px",
				}}
			>
				<div className="flex items-center gap-2">
					<span
						className="text-xs font-mono"
						style={{ color: "var(--code-muted)" }}
					>
						mermaid
					</span>
					<button
						type="button"
						onClick={() =>
							setViewMode((m) => (m === "diagram" ? "code" : "diagram"))
						}
						disabled={!isDiagramValid}
						className={cn(
							"p-1.5 rounded text-muted-foreground hover:text-foreground disabled:opacity-40 disabled:cursor-not-allowed",
							viewMode === "code" && "bg-primary/20 text-primary",
						)}
						title={
							isDiagramValid
								? viewMode === "diagram"
									? "Show code"
									: "Show diagram"
								: "Malformed Mermaid - showing code"
						}
						aria-label={
							isDiagramValid
								? viewMode === "diagram"
									? "Show code"
									: "Show diagram"
								: "Malformed Mermaid - showing code"
						}
					>
						<Code className="w-3.5 h-3.5" />
					</button>
				</div>
				<div className="flex items-center gap-1">
					{viewMode === "diagram" && isDiagramValid && (
						<>
							<button
								type="button"
								onClick={() =>
									setZoomLevel((z) => Math.max(ZOOM_MIN, z - ZOOM_STEP))
								}
								disabled={zoomLevel <= ZOOM_MIN}
								className="p-1.5 text-muted-foreground enabled:hover:text-foreground disabled:opacity-40"
								title="Zoom out"
							>
								<Minus className="w-3.5 h-3.5" />
							</button>
							<span className="text-[11px] tabular-nums text-muted-foreground min-w-10 text-center">
								{Math.round(zoomLevel * 100)}%
							</span>
							<button
								type="button"
								onClick={() =>
									setZoomLevel((z) => Math.min(ZOOM_MAX, z + ZOOM_STEP))
								}
								disabled={zoomLevel >= ZOOM_MAX}
								className="p-1.5 text-muted-foreground enabled:hover:text-foreground disabled:opacity-40"
								title="Zoom in"
							>
								<Plus className="w-3.5 h-3.5" />
							</button>
							<button
								type="button"
								onClick={() => setZoomLevel(1)}
								className="p-1.5 text-muted-foreground hover:text-foreground"
								title="Reset zoom"
							>
								<RotateCcw className="w-3.5 h-3.5" />
							</button>
						</>
					)}
					<CopyButton text={codeString} />
				</div>
			</div>

			{viewMode === "diagram" ? (
				<div className="p-4 overflow-auto bg-[var(--code-bg)] max-h-[70vh]">
					{isRendering && (
						<div className="text-xs text-muted-foreground">
							Rendering diagram…
						</div>
					)}
					{!isRendering && error && (
						<div className="text-xs text-destructive whitespace-pre-wrap">
							Failed to render Mermaid diagram: {error}
						</div>
					)}
					{!isRendering && !error && svg && (
						<div
							className="origin-top-left"
							style={{ width: `${zoomLevel * 100}%` }}
						>
							<div ref={svgContainerRef} aria-label="Mermaid diagram" />
						</div>
					)}
				</div>
			) : (
				<div className="overflow-x-auto">
					{!isDiagramValid && error && (
						<div className="px-3 py-2 text-xs text-destructive border-b border-destructive/20 bg-destructive/5">
							Malformed Mermaid: {error}
						</div>
					)}
					<SyntaxHighlighter
						style={
							(isDarkMode ? oneDark : oneLight) as Record<
								string,
								React.CSSProperties
							>
						}
						language="mermaid"
						PreTag="div"
						wrapLongLines={false}
						customStyle={{
							margin: 0,
							padding: "1rem",
							backgroundColor: "var(--code-bg)",
							fontSize: "0.75rem",
							minWidth: "fit-content",
						}}
					>
						{codeString}
					</SyntaxHighlighter>
				</div>
			)}
		</div>
	);
});

// Code block with theme awareness and auto-collapse for large blocks
const CodeBlockWithTheme = memo(function CodeBlockWithTheme({
	className,
	children,
}: {
	className?: string;
	children: React.ReactNode;
}) {
	const { resolvedTheme } = useTheme();
	const isDarkMode = resolvedTheme === "dark";

	const enableMermaid = useContext(MermaidEnabledContext);
	const match = /language-(\w+)/.exec(className || "");
	const codeString = normalizeCodeContent(children);
	const language = match ? match[1].toLowerCase() : "text";
	const isInline = !match && !codeString.includes("\n");
	const lineCount = codeString.split("\n").length;

	// Auto-collapse code blocks with more than 15 lines
	const shouldCollapse = lineCount > 15;
	const [isExpanded, setIsExpanded] = useState(!shouldCollapse);

	if (isInline) {
		return (
			<code
				className="px-1 py-0.5 rounded text-[0.85em] font-mono text-foreground/90 whitespace-normal break-words [overflow-wrap:anywhere]"
				style={{
					backgroundColor: "var(--code-inline-bg)",
				}}
			>
				{children}
			</code>
		);
	}

	if (enableMermaid && language === "mermaid") {
		return <MermaidCodeBlock codeString={codeString} />;
	}

	return (
		<div
			className="relative group my-3 overflow-hidden rounded-sm"
			style={{ borderColor: "var(--code-border)", borderWidth: "1px" }}
		>
			<div
				className="flex items-center justify-between px-3 py-1.5"
				style={{
					backgroundColor: "var(--code-bg)",
					borderBottomColor: "var(--code-border)",
					borderBottomWidth: "1px",
				}}
			>
				<div className="flex items-center gap-2">
					{shouldCollapse && (
						<button
							type="button"
							onClick={() => setIsExpanded(!isExpanded)}
							className="text-xs text-muted-foreground hover:text-foreground"
						>
							{isExpanded ? "[-]" : "[+]"}
						</button>
					)}
					<span
						className="text-xs font-mono"
						style={{ color: "var(--code-muted)" }}
					>
						{match ? match[1] : "plaintext"}
						{shouldCollapse && !isExpanded && (
							<span className="ml-2 text-muted-foreground">
								({lineCount} lines)
							</span>
						)}
					</span>
				</div>
				<CopyButton text={codeString} />
			</div>
			{isExpanded && (
				<div className="overflow-x-auto">
					<SyntaxHighlighter
						style={
							(isDarkMode ? oneDark : oneLight) as Record<
								string,
								React.CSSProperties
							>
						}
						language={match ? match[1] : "text"}
						PreTag="div"
						wrapLongLines={false}
						customStyle={{
							margin: 0,
							padding: "1rem",
							backgroundColor: "var(--code-bg)",
							fontSize: "0.75rem",
							minWidth: "fit-content",
						}}
					>
						{codeString}
					</SyntaxHighlighter>
				</div>
			)}
		</div>
	);
});

// remarkPlugins array - defined once
const remarkPlugins = [remarkGfm];

// Define components outside component to avoid recreation on every render
const markdownComponents: Components = {
	code({ className, children }) {
		return (
			<CodeBlockWithTheme className={className}>{children}</CodeBlockWithTheme>
		);
	},
	p({ children }) {
		return <p className="mb-3 last:mb-0 leading-relaxed">{children}</p>;
	},
	h1({ children }) {
		return (
			<h1 className="text-xl font-bold mb-3 mt-4 first:mt-0 text-foreground">
				{children}
			</h1>
		);
	},
	h2({ children }) {
		return (
			<h2 className="text-lg font-bold mb-2 mt-3 first:mt-0 text-foreground">
				{children}
			</h2>
		);
	},
	h3({ children }) {
		return (
			<h3 className="text-base font-semibold mb-2 mt-3 first:mt-0 text-foreground">
				{children}
			</h3>
		);
	},
	h4({ children }) {
		return (
			<h4 className="text-sm font-semibold mb-2 mt-2 first:mt-0 text-foreground">
				{children}
			</h4>
		);
	},
	ul({ children }) {
		return <ul className="list-none mb-3 space-y-1 pl-0">{children}</ul>;
	},
	ol({ children }) {
		return <ol className="list-none mb-3 space-y-1 pl-0">{children}</ol>;
	},
	li({ children, ordered, index }) {
		const marker = ordered ? `${(index ?? 0) + 1}.` : "•";
		return (
			<li className="flex items-start gap-2 text-foreground leading-relaxed">
				<span className="text-foreground/70 shrink-0">{marker}</span>
				<span className="min-w-0 flex-1 [&>p]:m-0 [&>p]:block">{children}</span>
			</li>
		);
	},
	blockquote({ children }) {
		return (
			<blockquote className="border-l-2 border-primary pl-4 py-1 my-3 text-muted-foreground italic">
				{children}
			</blockquote>
		);
	},
	a({ href, children }) {
		return (
			<a
				href={href}
				target="_blank"
				rel="noopener noreferrer"
				className="text-primary hover:text-primary/80 underline underline-offset-2"
			>
				{children}
			</a>
		);
	},
	table({ children }) {
		return (
			<div className="overflow-x-auto my-3">
				<table className="border border-border rounded-lg overflow-hidden text-xs w-max min-w-full">
					{children}
				</table>
			</div>
		);
	},
	thead({ children }) {
		return <thead className="bg-muted">{children}</thead>;
	},
	tbody({ children }) {
		return <tbody className="divide-y divide-border">{children}</tbody>;
	},
	tr({ children }) {
		return <tr className="divide-x divide-border">{children}</tr>;
	},
	th({ children }) {
		return (
			<th className="px-2 py-1.5 text-left text-xs font-semibold text-foreground whitespace-nowrap">
				{children}
			</th>
		);
	},
	td({ children }) {
		return (
			<td className="px-2 py-1.5 text-xs text-muted-foreground max-w-[200px]">
				{children}
			</td>
		);
	},
	hr() {
		return <hr className="my-4 border-border" />;
	},
	strong({ children }) {
		return (
			<strong className="font-semibold text-foreground">{children}</strong>
		);
	},
	em({ children }) {
		return <em className="italic text-muted-foreground">{children}</em>;
	},
};

// Pi can emit inline citation tokens like 【...†L1-L1】 that should not render.
const PI_CITATION_PATTERN = /【[^】]*[†‡✝✟][^】]*】/g;

function stripPiCitations(content: string) {
	return content
		.replace(PI_CITATION_PATTERN, "")
		.replace(/[ \t]+\n/g, "\n")
		.replace(/\n{3,}/g, "\n\n")
		.trimEnd();
}

function resolveHyphenationLang() {
	if (typeof document !== "undefined") {
		const htmlLang = document.documentElement.lang.trim();
		if (htmlLang.length > 0) {
			return htmlLang;
		}
	}

	if (typeof navigator !== "undefined" && navigator.language) {
		return navigator.language;
	}

	return "en";
}

export const MarkdownRenderer = memo(function MarkdownRenderer({
	content,
	className,
	enableMermaid = true,
}: MarkdownRendererProps) {
	const sanitizedContent = stripPiCitations(content);
	const hyphenationLang = resolveHyphenationLang();
	return (
		<div className={cn("markdown-content", className)} lang={hyphenationLang}>
			<MermaidEnabledContext.Provider value={enableMermaid}>
				<ReactMarkdown
					remarkPlugins={remarkPlugins}
					components={markdownComponents}
				>
					{sanitizedContent}
				</ReactMarkdown>
			</MermaidEnabledContext.Provider>
		</div>
	);
});

export { CopyButton };
