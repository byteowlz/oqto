/**
 * Hook for subscribing to session events with automatic transport selection.
 *
 * Uses WebSocket when features.websocket_events is enabled, falls back to SSE otherwise.
 * This provides a unified interface while allowing gradual migration.
 */

import {
	type LegacyEvent,
	type LegacyEventCallback,
	useWsSessionEvents,
} from "@/hooks/use-ws-session";
import { controlPlaneDirectBaseUrl } from "@/lib/control-plane-client";
import { subscribeToEvents } from "@/lib/opencode-client";
import { useCallback, useEffect, useRef, useState } from "react";

export type { LegacyEvent, LegacyEventCallback };

export type TransportMode = "sse" | "polling" | "ws" | "reconnecting";

export type UseSessionEventsOptions = {
	/** Whether WebSocket events are enabled (from features.websocket_events) */
	useWebSocket?: boolean;
	/** Workspace session ID for WebSocket subscriptions */
	workspaceSessionId?: string;
	/** Base URL for OpenCode API (used for SSE subscriptions) */
	opencodeBaseUrl?: string;
	/** Directory parameter for OpenCode requests */
	opencodeDirectory?: string;
	/** OpenCode session ID within the workspace session */
	activeSessionId?: string;
	/** Whether subscription is enabled */
	enabled?: boolean;
};

/**
 * Unified hook for subscribing to session events.
 *
 * Automatically selects between WebSocket and SSE based on feature flags.
 * Provides the same event interface regardless of transport.
 */
export function useSessionEvents(
	onEvent: LegacyEventCallback,
	options: UseSessionEventsOptions,
) {
	const {
		useWebSocket = false,
		workspaceSessionId,
		opencodeBaseUrl,
		opencodeDirectory,
		activeSessionId,
		enabled = true,
	} = options;

	// Keep callback ref stable
	const onEventRef = useRef(onEvent);
	onEventRef.current = onEvent;

	// Stable callback wrapper
	const handleEvent = useCallback((event: LegacyEvent) => {
		onEventRef.current(event);
	}, []);

	// Track transport mode for UI indicator
	const [transportMode, setTransportMode] = useState<TransportMode>(
		useWebSocket ? "ws" : "sse",
	);

	// WebSocket subscription (when enabled)
	const wsSession = useWsSessionEvents(
		useWebSocket ? workspaceSessionId : undefined,
		handleEvent,
		{
			enabled: useWebSocket && enabled && !!workspaceSessionId,
			opencodeBaseUrl,
			activeSessionId,
		},
	);

	// SSE subscription (when WebSocket is not enabled)
	useEffect(() => {
		// Skip if using WebSocket or not enabled
		if (useWebSocket || !enabled || !opencodeBaseUrl || !activeSessionId) {
			return;
		}

		const unsubscribe = subscribeToEvents(
			opencodeBaseUrl,
			(event) => {
				// Update transport mode from SSE events
				if (event.type === "transport.mode") {
					const props = event.properties as { mode?: "sse" | "polling" } | null;
					if (props?.mode) {
						setTransportMode(props.mode);
					}
				}
				handleEvent(event);
			},
			controlPlaneDirectBaseUrl(),
			{ directory: opencodeDirectory },
		);

		return unsubscribe;
	}, [
		useWebSocket,
		enabled,
		opencodeBaseUrl,
		activeSessionId,
		opencodeDirectory,
		handleEvent,
	]);

	// Update transport mode from WebSocket state
	useEffect(() => {
		if (useWebSocket) {
			setTransportMode(wsSession.transportMode);
		}
	}, [useWebSocket, wsSession.transportMode]);

	return {
		transportMode,
		isSubscribed: useWebSocket ? wsSession.isSubscribed : enabled,
		connectionState: wsSession.connectionState,
	};
}
