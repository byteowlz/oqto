/**
 * Sessions feature hooks.
 *
 * This module exports all hooks related to session management:
 * - WebSocket session subscriptions
 * - Session events
 * - OpenCode integration
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
	useOpenCodeSessions,
	useOpenCodeMessages,
	useCreateOpenCodeSession,
	useSendMessage,
	useAbortSession,
	useInvalidateOpenCode,
	openCodeKeys,
} from "./useOpenCode";

export {
	useWorkspaceSessions,
	useWorkspaceSession,
	useCreateWorkspaceSession,
	useStopWorkspaceSession,
	useRefreshWorkspaceSessions,
	workspaceSessionKeys,
} from "./useWorkspaceSessions";
