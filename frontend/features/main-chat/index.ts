/**
 * Main chat feature module.
 *
 * Provides main chat functionality including:
 * - Chat entry components
 * - Timeline display
 * - Settings views
 * - Navigation helpers
 */

// Hooks
export { useMainChatNavigation } from "./hooks";

// Components
export { ChatSearchBar } from "./components/ChatSearchBar";
export type { ChatSearchBarProps } from "./components/ChatSearchBar";
export { MainChatEntry } from "./components/MainChatEntry";
export type { MainChatEntryProps } from "./components/MainChatEntry";
export { MainChatPiView } from "./components/MainChatPiView";
export type { MainChatPiViewProps } from "./components/MainChatPiView";
export { MainChatSettingsView } from "./components/MainChatSettingsView";
export {
	MainChatTimeline,
	useActiveSessionTracker,
} from "./components/MainChatTimeline";
export type { MainChatTimelineProps } from "./components/MainChatTimeline";
