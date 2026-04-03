"use client";

import {
	type HstrySearchHit,
	type HstrySearchResponse,
	searchSessions,
} from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import {
	Bot,
	FileText,
	Loader2,
	MessageSquare,
	Search,
	User,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export type SearchMode = "sessions" | "messages";
// hstry indexes: pi and other adapters.
export type AgentFilter = "all" | "pi_agent";

interface SearchResultsProps {
	query: string;
	agentFilter: AgentFilter;
	locale: "en" | "de";
	onResultClick: (hit: HstrySearchHit) => void;
	extraHits?: HstrySearchHit[];
	className?: string;
}

function formatTimestamp(timestamp: number | undefined): string {
	if (!timestamp) return "";
	try {
		const date = new Date(timestamp);
		const now = new Date();
		const diffDays = Math.floor(
			(now.getTime() - date.getTime()) / (1000 * 60 * 60 * 24),
		);

		if (diffDays === 0) {
			return date.toLocaleTimeString([], {
				hour: "2-digit",
				minute: "2-digit",
			});
		}
		if (diffDays === 1) {
			return "Yesterday";
		}
		if (diffDays < 7) {
			return date.toLocaleDateString([], { weekday: "short" });
		}
		return date.toLocaleDateString([], { month: "short", day: "numeric" });
	} catch {
		return "";
	}
}

/**
 * Strip HTML tags, markdown syntax, and clean up whitespace from a text snippet.
 */
function cleanSnippet(raw: string): string {
	let text = raw;
	// Remove HTML tags
	text = text.replace(/<[^>]*>/g, "");
	// Remove markdown bold/italic
	text = text.replace(/\*{1,3}([^*]+)\*{1,3}/g, "$1");
	text = text.replace(/_{1,3}([^_]+)_{1,3}/g, "$1");
	// Remove markdown headers
	text = text.replace(/^#{1,6}\s+/gm, "");
	// Remove markdown links [text](url)
	text = text.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1");
	// Remove markdown images ![alt](url)
	text = text.replace(/!\[([^\]]*)\]\([^)]+\)/g, "$1");
	// Remove markdown code backticks
	text = text.replace(/```[^`]*```/gs, "[code]");
	text = text.replace(/`([^`]+)`/g, "$1");
	// Remove markdown list markers
	text = text.replace(/^[\s]*[-*+]\s+/gm, "");
	text = text.replace(/^[\s]*\d+\.\s+/gm, "");
	// Remove markdown blockquotes
	text = text.replace(/^>\s*/gm, "");
	// Remove markdown horizontal rules
	text = text.replace(/^[-*_]{3,}\s*$/gm, "");
	// Decode common HTML entities
	text = text.replace(/&amp;/g, "&");
	text = text.replace(/&lt;/g, "<");
	text = text.replace(/&gt;/g, ">");
	text = text.replace(/&quot;/g, '"');
	text = text.replace(/&#39;/g, "'");
	text = text.replace(/&nbsp;/g, " ");
	// Collapse multiple whitespace/newlines
	text = text.replace(/\s+/g, " ").trim();
	return text;
}

/**
 * Extract surrounding context from full content around a snippet match.
 * Returns ~5 lines before and after the match for context.
 */
function extractContext(
	content: string | undefined,
	snippet: string | undefined,
): string | null {
	if (!content || !snippet) return null;
	const cleaned = cleanSnippet(snippet).slice(0, 60);
	if (!cleaned) return null;

	const lines = content.split("\n");
	const lowerCleaned = cleaned.toLowerCase();

	// Find the line containing the snippet
	let matchLineIdx = -1;
	for (let i = 0; i < lines.length; i++) {
		if (lines[i].toLowerCase().includes(lowerCleaned)) {
			matchLineIdx = i;
			break;
		}
	}

	// If exact match fails, try first few words
	if (matchLineIdx === -1) {
		const words = lowerCleaned.split(/\s+/).slice(0, 4).join(" ");
		if (words.length > 10) {
			for (let i = 0; i < lines.length; i++) {
				if (lines[i].toLowerCase().includes(words)) {
					matchLineIdx = i;
					break;
				}
			}
		}
	}

	if (matchLineIdx === -1) {
		// Return first ~10 lines as fallback
		return cleanSnippet(lines.slice(0, 10).join("\n"));
	}

	const start = Math.max(0, matchLineIdx - 5);
	const end = Math.min(lines.length, matchLineIdx + 6);
	return lines.slice(start, end).join("\n");
}

/**
 * Highlight search query terms in text.
 */
function highlightTerms(text: string, query: string): React.ReactNode {
	if (!query.trim()) return text;
	const terms = query
		.trim()
		.split(/\s+/)
		.filter((t) => t.length > 1);
	if (terms.length === 0) return text;

	const pattern = terms
		.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
		.join("|");
	const regex = new RegExp(`(${pattern})`, "gi");
	const parts = text.split(regex);

	return parts.map((part, i) => {
		const key = `${i}-${part.slice(0, 16)}`;
		if (regex.test(part)) {
			return (
				<mark
					key={key}
					className="bg-primary/30 text-foreground rounded px-0.5"
				>
					{part}
				</mark>
			);
		}
		// Reset lastIndex since we're reusing the regex
		regex.lastIndex = 0;
		return part;
	});
}

function getAgentColor(agent: string): string {
	if (agent === "pi_agent") return "text-purple-500";
	return "text-muted-foreground";
}

/** Context preview popover on hover */
function ContextPopover({
	hit,
	query,
}: {
	hit: HstrySearchHit;
	query: string;
}) {
	const context = extractContext(hit.content, hit.snippet);
	if (!context) return null;

	const lines = context.split("\n");

	return (
		<div className="max-w-md max-h-64 overflow-auto p-3 space-y-1">
			{hit.title && (
				<div className="text-xs font-semibold text-foreground mb-2 pb-1 border-b border-border truncate">
					{hit.title}
				</div>
			)}
			<div className="font-mono text-[11px] leading-relaxed text-muted-foreground space-y-0.5">
				{lines.map((line, i) => {
					const cleaned = cleanSnippet(line);
					const key = `${i}-${cleaned.slice(0, 20)}`;
					if (!cleaned) return <div key={key} className="h-1" />;
					return (
						<div key={key} className="whitespace-pre-wrap break-words">
							{highlightTerms(cleaned, query)}
						</div>
					);
				})}
			</div>
		</div>
	);
}

export function SearchResults({
	query,
	agentFilter,
	locale,
	onResultClick,
	extraHits,
	className,
}: SearchResultsProps) {
	const { t } = useTranslation();
	const [results, setResults] = useState<HstrySearchResponse | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
	const hoverTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const resultListRef = useRef<HTMLDivElement>(null);

	// Debounced search (hstry-backed)
	// useeffect-guardrail: allow - async debounced fetch with cancellation
	useEffect(() => {
		if (!query.trim()) {
			setResults(null);
			setError(null);
			return;
		}

		const timer = setTimeout(async () => {
			setLoading(true);
			setError(null);
			try {
				const response = await searchSessions({
					q: query,
					agents: agentFilter,
					limit: 50,
				});
				const allHits = response.hits ?? [];

				// Sort by timestamp (most recent first)
				allHits.sort((a, b) => (b.timestamp ?? 0) - (a.timestamp ?? 0));

				setResults({ hits: allHits.slice(0, 50) });
			} catch (err) {
				setError(err instanceof Error ? err.message : t("search.error"));
				setResults(null);
			} finally {
				setLoading(false);
			}
		}, 300); // 300ms debounce

		return () => clearTimeout(timer);
	}, [query, agentFilter, t]);

	const handleMouseEnter = useCallback((index: number) => {
		if (hoverTimerRef.current) clearTimeout(hoverTimerRef.current);
		hoverTimerRef.current = setTimeout(() => {
			setHoveredIndex(index);
		}, 300); // 300ms hover delay
	}, []);

	const handleMouseLeave = useCallback(() => {
		if (hoverTimerRef.current) clearTimeout(hoverTimerRef.current);
		hoverTimerRef.current = null;
		setHoveredIndex(null);
	}, []);

	if (loading) {
		return (
			<div className={cn("flex items-center justify-center py-8", className)}>
				<Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
				<span className="ml-2 text-sm text-muted-foreground">
					{t("search.searching")}
				</span>
			</div>
		);
	}

	if (error) {
		return (
			<div className={cn("px-3 py-4 text-sm text-destructive", className)}>
				{error}
			</div>
		);
	}

	const mergedHits = (() => {
		const hits = results?.hits ?? [];
		const extras = extraHits ?? [];
		if (extras.length === 0) return hits;
		const seen = new Set(hits.map((hit) => hit.session_id ?? hit.source_path));
		const merged = [...hits];
		for (const hit of extras) {
			const key = hit.session_id ?? hit.source_path;
			if (key && seen.has(key)) continue;
			merged.push(hit);
			if (key) seen.add(key);
		}
		return merged;
	})();

	if (mergedHits.length === 0) {
		if (query.trim()) {
			return (
				<div
					className={cn(
						"px-3 py-4 text-sm text-muted-foreground text-center",
						className,
					)}
				>
					<Search className="w-8 h-8 mx-auto mb-2 opacity-50" />
					{t("search.noResults")}
				</div>
			);
		}
		return null;
	}

	return (
		<div
			className={cn("flex flex-col relative", className)}
			ref={resultListRef}
		>
			{mergedHits.map((hit, index) => (
				<div
					key={`${hit.source_path}-${hit.line_number ?? index}`}
					className="relative"
					onMouseEnter={() => handleMouseEnter(index)}
					onMouseLeave={handleMouseLeave}
				>
					<button
						type="button"
						onClick={() => onResultClick(hit)}
						className={cn(
							"w-full flex flex-col gap-0.5 px-3 py-2 text-left transition-colors",
							"hover:bg-sidebar-accent/50 border-b border-sidebar-border last:border-0",
						)}
					>
						{/* Header: role icon + timestamp */}
						<div className="flex items-center justify-between gap-2">
							<div className="flex items-center gap-1.5">
								{hit.role === "user" ? (
									<User className="w-3 h-3 text-muted-foreground" />
								) : (
									<Bot className={cn("w-3 h-3", getAgentColor(hit.agent))} />
								)}
								{hit.workspace && (
									<span className="text-[10px] text-muted-foreground/70 truncate max-w-[140px]">
										{hit.workspace.replace(/^\/home\/[^/]+\//, "~/")}
									</span>
								)}
							</div>
							<span className="text-[10px] text-muted-foreground flex-shrink-0">
								{formatTimestamp(hit.timestamp)}
							</span>
						</div>

						{/* Title */}
						{hit.title && (
							<div className="text-xs font-medium text-foreground truncate">
								{highlightTerms(hit.title, query)}
							</div>
						)}

						{/* Clean snippet */}
						{(hit.snippet || hit.title) && (
							<div className="text-[11px] text-muted-foreground line-clamp-2 leading-relaxed">
								{highlightTerms(
									cleanSnippet(hit.snippet || hit.title || ""),
									query,
								)}
							</div>
						)}
					</button>

					{/* Hover context popover */}
					{hoveredIndex === index && hit.content && (
						<div
							className={cn(
								"absolute left-full top-0 ml-2 z-50",
								"bg-popover border border-border rounded-lg shadow-xl",
								"animate-in fade-in-0 zoom-in-95 duration-150",
							)}
							onMouseEnter={() => {
								// Keep popover open when hovering over it
								if (hoverTimerRef.current) clearTimeout(hoverTimerRef.current);
							}}
							onMouseLeave={handleMouseLeave}
						>
							<ContextPopover hit={hit} query={query} />
						</div>
					)}
				</div>
			))}
		</div>
	);
}
