"use client";

import {
	type HstrySearchHit,
	type HstrySearchResponse,
	searchSessions,
} from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import { Bot, Loader2, MessageSquare, Search, User } from "lucide-react";
import { useEffect, useState } from "react";

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

const t = {
	en: {
		noResults: "No results found",
		searching: "Searching...",
		piAgent: "Chat",
		user: "User",
		assistant: "Assistant",
		error: "Search failed",
	},
	de: {
		noResults: "Keine Ergebnisse gefunden",
		searching: "Suche...",
		piAgent: "Chat",
		user: "Benutzer",
		assistant: "Assistent",
		error: "Suche fehlgeschlagen",
	},
};

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

function getAgentLabel(agent: string, locale: "en" | "de"): string {
	if (agent === "pi_agent") return t[locale].piAgent;
	return agent;
}

function getAgentColor(agent: string): string {
	if (agent === "pi_agent") return "text-purple-500";
	return "text-muted-foreground";
}

export function SearchResults({
	query,
	agentFilter,
	locale,
	onResultClick,
	extraHits,
	className,
}: SearchResultsProps) {
	const [results, setResults] = useState<HstrySearchResponse | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);

	// Debounced search (hstry-backed)
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
				setError(err instanceof Error ? err.message : t[locale].error);
				setResults(null);
			} finally {
				setLoading(false);
			}
		}, 300); // 300ms debounce

		return () => clearTimeout(timer);
	}, [query, agentFilter, locale]);

	if (loading) {
		return (
			<div className={cn("flex items-center justify-center py-8", className)}>
				<Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
				<span className="ml-2 text-sm text-muted-foreground">
					{t[locale].searching}
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
					{t[locale].noResults}
				</div>
			);
		}
		return null;
	}

	return (
		<div className={cn("flex flex-col", className)}>
			{mergedHits.map((hit, index) => (
				<button
					key={`${hit.source_path}-${hit.line_number ?? index}`}
					type="button"
					onClick={() => onResultClick(hit)}
					className={cn(
						"flex flex-col gap-1 px-3 py-2 text-left transition-colors",
						"hover:bg-sidebar-accent/50 border-b border-sidebar-border last:border-0",
					)}
				>
					{/* Header: agent badge + timestamp */}
					<div className="flex items-center justify-between gap-2">
						<div className="flex items-center gap-1.5">
							<MessageSquare
								className={cn("w-3 h-3", getAgentColor(hit.agent))}
							/>
							<span
								className={cn(
									"text-[10px] font-medium uppercase tracking-wide",
									getAgentColor(hit.agent),
								)}
							>
								{getAgentLabel(hit.agent, locale)}
							</span>
							{hit.role && (
								<span className="text-[10px] text-muted-foreground">
									{hit.role === "user" ? (
										<User className="w-3 h-3 inline" />
									) : (
										<Bot className="w-3 h-3 inline" />
									)}
								</span>
							)}
						</div>
						<span className="text-[10px] text-muted-foreground">
							{formatTimestamp(hit.timestamp)}
						</span>
					</div>

					{/* Title if available */}
					{hit.title && (
						<div className="text-xs font-medium text-foreground truncate">
							{hit.title}
						</div>
					)}

					{/* Snippet or title fallback */}
					{(hit.snippet || hit.title) && (
						<div className="text-xs text-muted-foreground line-clamp-2">
							{(hit.snippet || hit.title || "").replace(/\*\*/g, "")}
						</div>
					)}

					{/* Workspace path (truncated) */}
					{hit.workspace && (
						<div className="text-[10px] text-muted-foreground/50 truncate">
							{hit.workspace.replace(/^\/home\/[^/]+\//, "~/")}
						</div>
					)}
				</button>
			))}
		</div>
	);
}
