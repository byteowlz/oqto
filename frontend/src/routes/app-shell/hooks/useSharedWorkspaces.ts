/**
 * Hook for managing shared workspaces data and state.
 * Listens to system WS events for real-time membership/workspace changes.
 */
import {
	type SharedWorkspaceInfo,
	listSharedWorkspaces,
} from "@/lib/api/shared-workspaces";
import { getWsManager } from "@/lib/ws-manager";
import type { SystemWsEvent } from "@/lib/ws-mux-types";
import { useCallback, useEffect, useRef, useState } from "react";

export interface UseSharedWorkspacesResult {
	/** All shared workspaces the user belongs to. */
	sharedWorkspaces: SharedWorkspaceInfo[];
	/** Loading state for initial fetch. */
	loading: boolean;
	/** Error message if fetch failed. */
	error: string | null;
	/** Re-fetch shared workspaces from the backend. */
	refresh: () => Promise<void>;
	/** Currently expanded workspace IDs in the sidebar. */
	expandedWorkspaces: Set<string>;
	/** Toggle expansion of a workspace in the sidebar. */
	toggleWorkspaceExpanded: (workspaceId: string) => void;
}

export function useSharedWorkspaces(): UseSharedWorkspacesResult {
	const [sharedWorkspaces, setSharedWorkspaces] = useState<
		SharedWorkspaceInfo[]
	>([]);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [expandedWorkspaces, setExpandedWorkspaces] = useState<Set<string>>(
		() => new Set(),
	);
	const mountedRef = useRef(true);

	const refresh = useCallback(async () => {
		try {
			const data = await listSharedWorkspaces();
			if (mountedRef.current) {
				setSharedWorkspaces(data);
				setError(null);
			}
		} catch (err) {
			if (mountedRef.current) {
				setError(
					err instanceof Error
						? err.message
						: "Failed to load shared workspaces",
				);
			}
		} finally {
			if (mountedRef.current) {
				setLoading(false);
			}
		}
	}, []);

	// Initial fetch
	useEffect(() => {
		mountedRef.current = true;
		refresh();
		return () => {
			mountedRef.current = false;
		};
	}, [refresh]);

	// Listen for real-time shared_workspace.updated events via WebSocket
	useEffect(() => {
		const manager = getWsManager();
		const unsub = manager.subscribe("system", (event) => {
			const sysEvent = event as SystemWsEvent;
			if (sysEvent.type === "shared_workspace.updated") {
				// Re-fetch the full list on any change
				refresh();
			}
		});
		return unsub;
	}, [refresh]);

	const toggleWorkspaceExpanded = useCallback((workspaceId: string) => {
		setExpandedWorkspaces((prev) => {
			const next = new Set(prev);
			if (next.has(workspaceId)) {
				next.delete(workspaceId);
			} else {
				next.add(workspaceId);
			}
			return next;
		});
	}, []);

	return {
		sharedWorkspaces,
		loading,
		error,
		refresh,
		expandedWorkspaces,
		toggleWorkspaceExpanded,
	};
}
