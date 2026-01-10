/**
 * A2UI Surface Hook
 *
 * React hook for managing A2UI surface state with WebSocket integration
 */

import { A2UISurfaceManager } from "@/lib/a2ui/state";
import type {
	A2UIMessage,
	A2UISurfaceState,
	A2UIUserAction,
} from "@/lib/a2ui/types";
import { setValueAtPath } from "@/lib/a2ui/types";
import { useCallback, useEffect, useRef, useState } from "react";

export interface UseA2UISurfaceOptions {
	/** Initial A2UI messages to process */
	initialMessages?: A2UIMessage[];
	/** Callback when user performs an action */
	onAction?: (action: A2UIUserAction) => void;
	/** Surface ID (if managing a single surface) */
	surfaceId?: string;
}

export interface UseA2UISurfaceResult {
	/** The surface state (if surfaceId was provided) */
	surface: A2UISurfaceState | undefined;
	/** All surfaces managed by this hook */
	surfaces: Map<string, A2UISurfaceState>;
	/** Process new A2UI messages */
	processMessages: (messages: A2UIMessage[]) => void;
	/** Handle user action */
	handleAction: (action: A2UIUserAction) => void;
	/** Handle data model change (for two-way binding) */
	handleDataChange: (surfaceId: string, path: string, value: unknown) => void;
	/** Clear all surfaces */
	clear: () => void;
	/** Check if any surface is ready */
	hasReadySurfaces: boolean;
}

/**
 * Hook for managing A2UI surface state
 */
export function useA2UISurface(
	options: UseA2UISurfaceOptions = {},
): UseA2UISurfaceResult {
	const { initialMessages, onAction, surfaceId } = options;

	// Use ref to maintain manager instance across renders
	const managerRef = useRef<A2UISurfaceManager | null>(null);
	if (!managerRef.current) {
		managerRef.current = new A2UISurfaceManager();
	}

	// State to trigger re-renders when surfaces change
	const [version, setVersion] = useState(0);

	// Subscribe to surface changes
	useEffect(() => {
		const manager = managerRef.current;
		if (!manager) return;

		const unsubscribe = manager.subscribe(() => {
			setVersion((v) => v + 1);
		});

		return unsubscribe;
	}, []);

	// Process initial messages
	useEffect(() => {
		if (initialMessages && initialMessages.length > 0) {
			managerRef.current?.processMessages(initialMessages);
		}
	}, [initialMessages]);

	// Process new messages
	const processMessages = useCallback((messages: A2UIMessage[]) => {
		managerRef.current?.processMessages(messages);
	}, []);

	// Handle user action
	const handleAction = useCallback(
		(action: A2UIUserAction) => {
			if (onAction) {
				onAction(action);
			}
		},
		[onAction],
	);

	// Handle data model change (two-way binding)
	const handleDataChange = useCallback(
		(targetSurfaceId: string, path: string, value: unknown) => {
			const manager = managerRef.current;
			if (!manager) return;

			const surface = manager.getSurface(targetSurfaceId);
			if (surface) {
				// Update the data model in place
				setValueAtPath(surface.dataModel, path, value);
				// Trigger re-render
				setVersion((v) => v + 1);
			}
		},
		[],
	);

	// Clear all surfaces
	const clear = useCallback(() => {
		managerRef.current?.clear();
	}, []);

	// Get current surfaces
	const manager = managerRef.current;
	const surfaces = manager?.getAllSurfaces() ?? new Map();
	const surface = surfaceId ? manager?.getSurface(surfaceId) : undefined;

	// Check if any surface is ready
	const hasReadySurfaces = Array.from(surfaces.values()).some((s) => s.isReady);

	return {
		surface,
		surfaces,
		processMessages,
		handleAction,
		handleDataChange,
		clear,
		hasReadySurfaces,
	};
}
