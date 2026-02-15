/**
 * Sessions feature hooks.
 *
 * This module exports all hooks related to session management:
 * - WebSocket session subscriptions
 * - Session events
 * - Workspace sessions
 */

export {
	useWsConnection,
	useWsSession,
	useWsSessionWithConnection,
	useWsSessionEvents,
} from "./useWsSession";
export type {
	SessionEvent,
	SessionEventCallback,
	Permission,
} from "./useWsSession";

export { useSessionEvents } from "./useSessionEvents";
export type {
	TransportMode,
	UseSessionEventsOptions,
} from "./useSessionEvents";

export {
	useWorkspaceSessions,
	useWorkspaceSession,
	useCreateWorkspaceSession,
	useStopWorkspaceSession,
	useRefreshWorkspaceSessions,
	workspaceSessionKeys,
} from "./useWorkspaceSessions";
