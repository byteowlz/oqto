/**
 * Hook for searching within a specific Pi session using hstry.
 */

import {
	type InSessionSearchResult,
	searchInPiSession,
} from "@/lib/api";
import { useCallback, useMemo, useState } from "react";

export type UseSessionSearchOptions = {
	/** Session ID to search within */
	sessionId: string | null;
	/** Debounce delay in ms (default: 300) */
	debounceMs?: number;
	/** Maximum results (default: 20) */
	limit?: number;
};

export type UseSessionSearchResult = {
	/** Current search query */
	query: string;
	/** Set search query */
	setQuery: (query: string) => void;
	/** Search results */
	results: InSessionSearchResult[];
	/** Whether search is in progress */
	isSearching: boolean;
	/** Error message if search failed */
	error: string | null;
	/** Clear search state */
	clearSearch: () => void;
	/** Whether search is active (has query) */
	isActive: boolean;
};

/**
 * Hook for searching within a Pi session.
 * Provides debounced search with loading state.
 */
export function useSessionSearch(
	options: UseSessionSearchOptions,
): UseSessionSearchResult {
	const { sessionId, debounceMs = 300, limit = 20 } = options;

	const [query, setQueryRaw] = useState("");
	const [results, setResults] = useState<InSessionSearchResult[]>([]);
	const [isSearching, setIsSearching] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [debounceTimer, setDebounceTimer] = useState<ReturnType<
		typeof setTimeout
	> | null>(null);

	const executeSearch = useCallback(
		async (searchQuery: string) => {
			if (!sessionId || !searchQuery.trim()) {
				setResults([]);
				setError(null);
				return;
			}

			setIsSearching(true);
			setError(null);

			try {
				const searchResults = await searchInPiSession(
					sessionId,
					searchQuery,
					limit,
				);
				setResults(searchResults);
			} catch (err) {
				const message = err instanceof Error ? err.message : "Search failed";
				setError(message);
				setResults([]);
			} finally {
				setIsSearching(false);
			}
		},
		[sessionId, limit],
	);

	const setQuery = useCallback(
		(newQuery: string) => {
			setQueryRaw(newQuery);

			// Clear previous timer
			if (debounceTimer) {
				clearTimeout(debounceTimer);
			}

			if (!newQuery.trim()) {
				setResults([]);
				setError(null);
				setIsSearching(false);
				return;
			}

			// Set new debounced search
			const timer = setTimeout(() => {
				executeSearch(newQuery);
			}, debounceMs);
			setDebounceTimer(timer);
		},
		[debounceMs, debounceTimer, executeSearch],
	);

	const clearSearch = useCallback(() => {
		if (debounceTimer) {
			clearTimeout(debounceTimer);
		}
		setQueryRaw("");
		setResults([]);
		setError(null);
		setIsSearching(false);
	}, [debounceTimer]);

	const isActive = useMemo(() => query.trim().length > 0, [query]);

	return {
		query,
		setQuery,
		results,
		isSearching,
		error,
		clearSearch,
		isActive,
	};
}
