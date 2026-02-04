/**
 * Chat feature module.
 *
 * Provides default chat functionality including:
 * - Chat entry components
 * - Timeline display
 * - Settings views
 * - Navigation helpers
 */

// Hooks
export { useChatNavigation } from "./hooks";

// Components
export { ChatSearchBar } from "./components/ChatSearchBar";
export type { ChatSearchBarProps } from "./components/ChatSearchBar";
export { ChatEntry } from "./components/ChatEntry";
export type { ChatEntryProps } from "./components/ChatEntry";
export { ChatView } from "./components/ChatView";
export type { ChatViewProps } from "./components/ChatView";
export { ChatSettingsView } from "./components/ChatSettingsView";
export { PiSettingsView } from "./components/PiSettingsView";
export type { PiSettingsViewProps } from "./components/PiSettingsView";
export {
	ChatTimeline,
	useActiveSessionTracker,
} from "./components/ChatTimeline";
export type { ChatTimelineProps } from "./components/ChatTimeline";
