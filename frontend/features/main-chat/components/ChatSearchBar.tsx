"use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useSessionSearch } from "@/hooks/use-session-search";
import { cn } from "@/lib/utils";
import { ChevronDown, ChevronUp, Loader2, Search, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

export type ChatSearchBarProps = {
	/** Session ID to search within */
	sessionId: string | null;
	/** Callback when a result is selected */
	onResultSelect: (result: { lineNumber: number; messageId?: string }) => void;
	/** Class name for container */
	className?: string;
	/** Whether search is expanded/visible */
	isOpen: boolean;
	/** Callback to toggle search visibility */
	onToggle: () => void;
	/** Locale for translations */
	locale?: "en" | "de";
	/** Hide the close button (when parent provides its own) */
	hideCloseButton?: boolean;
};

const translations = {
	en: {
		placeholder: "Search in conversation...",
		noResults: "No results",
		results: (count: number) => `${count} result${count !== 1 ? "s" : ""}`,
		close: "Close search",
		prev: "Previous result",
		next: "Next result",
	},
	de: {
		placeholder: "In Konversation suchen...",
		noResults: "Keine Ergebnisse",
		results: (count: number) => `${count} Ergebnis${count !== 1 ? "se" : ""}`,
		close: "Suche schliessen",
		prev: "Vorheriges Ergebnis",
		next: "Nachstes Ergebnis",
	},
};

/**
 * Search bar component for searching within a chat session.
 * Shows inline results with navigation.
 */
export function ChatSearchBar({
	sessionId,
	onResultSelect,
	className,
	isOpen,
	onToggle,
	locale = "en",
	hideCloseButton = false,
}: ChatSearchBarProps) {
	const t = translations[locale];
	const inputRef = useRef<HTMLInputElement>(null);
	const [currentResultIndex, setCurrentResultIndex] = useState(0);

	const {
		query,
		setQuery,
		results,
		isSearching,
		error,
		clearSearch,
		isActive,
	} = useSessionSearch({
		sessionId,
		debounceMs: 300,
		limit: 50,
	});

	// Focus input when opened
	useEffect(() => {
		if (isOpen && inputRef.current) {
			inputRef.current.focus();
		}
	}, [isOpen]);

	// Reset index when query changes - use ref to avoid dependency issues
	const prevQueryRef = useRef(query);
	if (prevQueryRef.current !== query) {
		prevQueryRef.current = query;
		if (currentResultIndex !== 0) {
			setCurrentResultIndex(0);
		}
	}

	// Navigate to current result
	const currentResult = results[currentResultIndex];
	useEffect(() => {
		if (currentResult) {
			onResultSelect({
				lineNumber: currentResult.line_number,
				messageId: currentResult.message_id,
			});
		}
	}, [currentResult, onResultSelect]);

	const handleClose = useCallback(() => {
		clearSearch();
		onToggle();
	}, [clearSearch, onToggle]);

	const handlePrev = useCallback(() => {
		if (results.length === 0) return;
		setCurrentResultIndex((prev) => (prev > 0 ? prev - 1 : results.length - 1));
	}, [results.length]);

	const handleNext = useCallback(() => {
		if (results.length === 0) return;
		setCurrentResultIndex((prev) => (prev < results.length - 1 ? prev + 1 : 0));
	}, [results.length]);

	const handleKeyDown = useCallback(
		(e: React.KeyboardEvent) => {
			if (e.key === "Escape") {
				handleClose();
			} else if (e.key === "Enter") {
				if (e.shiftKey) {
					handlePrev();
				} else {
					handleNext();
				}
				e.preventDefault();
			}
		},
		[handleClose, handleNext, handlePrev],
	);

	if (!isOpen) {
		return (
			<Button
				variant="ghost"
				size="icon"
				onClick={onToggle}
				className={cn("h-8 w-8", className)}
				title="Search (Ctrl+F)"
			>
				<Search className="h-4 w-4" />
			</Button>
		);
	}

	return (
		<div className={cn("flex items-center gap-2 p-2 bg-muted/30", className)}>
			<Search className="h-4 w-4 text-muted-foreground flex-shrink-0" />
			<Input
				ref={inputRef}
				type="text"
				value={query}
				onChange={(e) => setQuery(e.target.value)}
				onKeyDown={handleKeyDown}
				placeholder={t.placeholder}
				className="h-7 text-sm border-0 bg-transparent shadow-none focus-visible:ring-0 focus-visible:ring-offset-0"
			/>

			{/* Results indicator */}
			{isActive && (
				<div className="flex items-center gap-1 text-xs text-muted-foreground flex-shrink-0">
					{isSearching ? (
						<Loader2 className="h-3 w-3 animate-spin" />
					) : results.length > 0 ? (
						<span>
							{currentResultIndex + 1}/{results.length}
						</span>
					) : (
						<span>{t.noResults}</span>
					)}
				</div>
			)}

			{/* Navigation buttons */}
			{results.length > 0 && (
				<div className="flex items-center gap-0.5 flex-shrink-0">
					<Button
						variant="ghost"
						size="icon"
						onClick={handlePrev}
						className="h-6 w-6"
						title={t.prev}
					>
						<ChevronUp className="h-3 w-3" />
					</Button>
					<Button
						variant="ghost"
						size="icon"
						onClick={handleNext}
						className="h-6 w-6"
						title={t.next}
					>
						<ChevronDown className="h-3 w-3" />
					</Button>
				</div>
			)}

			{/* Close button */}
			{!hideCloseButton && (
				<Button
					variant="ghost"
					size="icon"
					onClick={handleClose}
					className="h-6 w-6 flex-shrink-0"
					title={t.close}
				>
					<X className="h-3 w-3" />
				</Button>
			)}
		</div>
	);
}
