/**
 * Sessions feature module.
 *
 * Provides session management functionality including:
 * - WebSocket session connections
 * - Session event subscriptions
 * - Workspace session management
 */

// Hooks
export {
	// WebSocket session
	useWsConnection,
	useWsSession,
	useWsSessionWithConnection,
	useWsSessionEvents,
	type SessionEvent,
	type SessionEventCallback,
	type Permission,
	// Session events
	useSessionEvents,
	type TransportMode,
	type UseSessionEventsOptions,
	// Workspace sessions
	useWorkspaceSessions,
	useWorkspaceSession,
	useCreateWorkspaceSession,
	useStopWorkspaceSession,
	useRefreshWorkspaceSessions,
	workspaceSessionKeys,
} from "./hooks";

// Components
export { SessionScreen } from "./SessionScreen";
export type { ChatInputAreaHandle } from "./components/ChatInputArea";
export { ChatInputArea } from "./components/ChatInputArea";
export { AgentSettingsView } from "./components/AgentSettingsView";
export { BrowserView } from "./components/BrowserView";
export { CanvasView } from "./components/CanvasView";
export { FileTreeView } from "./components/FileTreeView";
export { MemoriesView } from "./components/MemoriesView";
export { PreviewView } from "./components/PreviewView";
export { TerminalView } from "./components/TerminalView";
export { TrxView } from "./components/TrxView";
