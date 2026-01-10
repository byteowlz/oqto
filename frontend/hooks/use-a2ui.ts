/**
 * A2UI Hook - Manages A2UI surface state and WebSocket integration
 *
 * This hook can be used by any component that needs to display A2UI surfaces.
 */

import type { A2UIMessage, A2UIUserAction } from "@/lib/a2ui/types";
import { useCallback, useEffect, useRef, useState } from "react";

export interface A2UISurfaceState {
	surfaceId: string;
	sessionId: string;
	messages: A2UIMessage[];
	blocking: boolean;
	requestId?: string;
	createdAt: Date;
	/** ID of the message this surface is anchored to */
	anchorMessageId?: string;
	/** Whether user has answered */
	answered?: boolean;
	/** The action name that was selected */
	answeredAction?: string;
	/** When the user answered */
	answeredAt?: Date;
}

export interface UseA2UIOptions {
	/** Filter surfaces to a specific session ID */
	sessionId?: string;
	/** Callback when a new surface arrives (for auto-scroll, etc.) */
	onSurfaceReceived?: (surface: A2UISurfaceState) => void;
}

export interface UseA2UIResult {
	/** All A2UI surfaces */
	surfaces: A2UISurfaceState[];
	/** Handle user action on a surface */
	handleAction: (action: A2UIUserAction) => void;
	/** Dismiss a surface */
	handleDismiss: (surfaceId: string) => void;
	/** Clear all surfaces */
	clearSurfaces: () => void;
	/** Get surfaces anchored to a specific message */
	getSurfacesForMessage: (messageId: string) => A2UISurfaceState[];
	/** Get surfaces for the last assistant message (fallback) */
	getUnanchoredSurfaces: () => A2UISurfaceState[];
}

/**
 * Find the last assistant message ID from a messages array
 */
export function findLastAssistantMessageId(
	messages: Array<{ info: { id: string; role: string } }>,
): string | undefined {
	for (let i = messages.length - 1; i >= 0; i--) {
		if (messages[i].info.role === "assistant") {
			return messages[i].info.id;
		}
	}
	return undefined;
}

/**
 * Hook for managing A2UI surfaces
 */
export function useA2UI(
	messagesRef: React.RefObject<Array<{ info: { id: string; role: string } }>>,
	options: UseA2UIOptions = {},
): UseA2UIResult {
	const { sessionId, onSurfaceReceived } = options;
	const [surfaces, setSurfaces] = useState<A2UISurfaceState[]>([]);

	// Subscribe to WebSocket A2UI events
	useEffect(() => {
		let unsubscribe: (() => void) | undefined;

		import("@/lib/ws-client").then(({ getWsClient }) => {
			const client = getWsClient();
			unsubscribe = client.onEvent((event) => {
				if (event.type === "a2ui_surface") {
					const e = event as {
						session_id: string;
						surface_id: string;
						messages: unknown[];
						blocking?: boolean;
						request_id?: string;
					};

					// Filter by session if specified
					if (sessionId && e.session_id !== sessionId) {
						return;
					}

					console.log("[A2UI] Surface received via WS:", e.surface_id);
					const now = new Date();

					// Find anchor message
					const currentMessages = messagesRef.current || [];
					const anchorId = findLastAssistantMessageId(currentMessages);
					console.log("[A2UI] Anchoring to message:", anchorId);

					const newSurface: A2UISurfaceState = {
						surfaceId: e.surface_id,
						sessionId: e.session_id,
						messages: e.messages as A2UIMessage[],
						blocking: e.blocking ?? false,
						requestId: e.request_id,
						createdAt: now,
						anchorMessageId: anchorId,
					};

					setSurfaces((prev) => {
						const existing = prev.findIndex(
							(s) => s.surfaceId === e.surface_id,
						);
						if (existing >= 0) {
							const updated = [...prev];
							// Keep original createdAt and anchor when updating
							updated[existing] = {
								...newSurface,
								createdAt: prev[existing].createdAt,
								anchorMessageId: prev[existing].anchorMessageId,
							};
							return updated;
						}
						return [...prev, newSurface];
					});

					onSurfaceReceived?.(newSurface);
				} else if (event.type === "a2ui_action_resolved") {
					const e = event as { request_id: string };
					console.log("[A2UI] Action resolved:", e.request_id);
					setSurfaces((prev) =>
						prev.map((s) =>
							s.requestId === e.request_id
								? { ...s, answered: true, answeredAt: new Date() }
								: s,
						),
					);
				}
			});
		});

		return () => {
			unsubscribe?.();
		};
	}, [sessionId, onSurfaceReceived, messagesRef]);

	// Handle user action
	const handleAction = useCallback(
		(action: A2UIUserAction) => {
			const surface = surfaces.find((s) => s.surfaceId === action.surfaceId);
			if (!surface) {
				console.warn("[A2UI] Surface not found for action:", action.surfaceId);
				return;
			}

			console.log("[A2UI] Sending action:", action);

			import("@/lib/ws-client").then(({ getWsClient }) => {
				const wsClient = getWsClient();
				wsClient.sendA2UIAction(
					surface.sessionId,
					action.surfaceId,
					action.name,
					action.sourceComponentId,
					action.context,
					surface.requestId,
				);
			});

			// Mark as answered immediately (optimistic update)
			setSurfaces((prev) =>
				prev.map((s) =>
					s.surfaceId === action.surfaceId
						? {
								...s,
								answered: true,
								answeredAction: action.name,
								answeredAt: new Date(),
							}
						: s,
				),
			);
		},
		[surfaces],
	);

	// Dismiss a surface
	const handleDismiss = useCallback((surfaceId: string) => {
		setSurfaces((prev) => prev.filter((s) => s.surfaceId !== surfaceId));
	}, []);

	// Clear all surfaces
	const clearSurfaces = useCallback(() => {
		setSurfaces([]);
	}, []);

	// Get surfaces for a specific message
	const getSurfacesForMessage = useCallback(
		(messageId: string) => {
			return surfaces.filter((s) => s.anchorMessageId === messageId);
		},
		[surfaces],
	);

	// Get surfaces without a valid anchor (fallback to last message)
	const getUnanchoredSurfaces = useCallback(() => {
		return surfaces.filter((s) => !s.anchorMessageId);
	}, [surfaces]);

	return {
		surfaces,
		handleAction,
		handleDismiss,
		clearSurfaces,
		getSurfacesForMessage,
		getUnanchoredSurfaces,
	};
}

/**
 * Create segments with A2UI surfaces interleaved based on timestamps
 */
export function interleaveA2UISurfaces<T extends { timestamp?: number }>(
	segments: T[],
	a2uiSurfaces: A2UISurfaceState[],
): Array<T | { type: "a2ui"; surface: A2UISurfaceState; timestamp: number }> {
	const result: Array<
		T | { type: "a2ui"; surface: A2UISurfaceState; timestamp: number }
	> = [...segments];

	// Add A2UI surfaces with their timestamps
	for (const surface of a2uiSurfaces) {
		result.push({
			type: "a2ui" as const,
			surface,
			timestamp: surface.createdAt.getTime(),
		});
	}

	// Sort by timestamp
	result.sort((a, b) => {
		const aTime = "timestamp" in a ? a.timestamp || 0 : 0;
		const bTime = "timestamp" in b ? b.timestamp || 0 : 0;
		return aTime - bTime;
	});

	return result;
}
