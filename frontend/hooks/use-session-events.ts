/**
 * Hook for subscribing to session events over WebSocket.
 */

import { useWsSessionEvents } from "@/hooks/use-ws-session";
import type { WsEvent } from "@/lib/ws-client";
import { useCallback, useEffect, useRef, useState } from "react";

export type TransportMode = "sse" | "polling" | "ws" | "reconnecting";

export type UseSessionEventsOptions = {
	/** Whether WebSocket events are enabled (from features.websocket_events) */
	useWebSocket?: boolean;
	/** Workspace session ID for WebSocket subscriptions */
	workspaceSessionId?: string;
	/** Whether subscription is enabled */
	enabled?: boolean;
};

/**
 * Unified hook for subscribing to session events.
 *
 * Provides a single event interface from the WebSocket transport.
 */
export function useSessionEvents(
	onEvent: (event: WsEvent) => void,
	options: UseSessionEventsOptions,
) {
	const { useWebSocket = true, workspaceSessionId, enabled = true } = options;

	// Keep callback ref stable
	const onEventRef = useRef(onEvent);
	onEventRef.current = onEvent;

	// Stable callback wrapper
	const handleEvent = useCallback((event: WsEvent) => {
		onEventRef.current(event);
	}, []);

	// Track transport mode for UI indicator
	const [transportMode, setTransportMode] = useState<TransportMode>("ws");

	// WebSocket subscription (when enabled)
	const wsSession = useWsSessionEvents(
		useWebSocket ? workspaceSessionId : undefined,
		handleEvent,
		{
			enabled: useWebSocket && enabled && !!workspaceSessionId,
		},
	);

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
