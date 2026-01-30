/**
 * @deprecated Import from @/features/sessions instead
 * This file re-exports for backwards compatibility
 */
export {
	useWsConnection,
	useWsSession,
	useWsSessionWithConnection,
	useWsSessionEvents,
	type SessionEvent,
	type SessionEventCallback,
	type Permission,
} from "@/features/sessions/hooks/useWsSession";
