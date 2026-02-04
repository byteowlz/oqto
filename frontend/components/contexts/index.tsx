export {
	UIProvider,
	useUIContext,
	useLocale,
	useActiveApp,
} from "./ui-context";

export {
	UIControlProvider,
	useUIControl,
} from "./ui-control-context";

// Workspace context exports
export {
	WorkspaceProvider,
	useWorkspaceContext,
	useWorkspaceSessions,
	type WorkspaceContextValue,
} from "./workspace-context";

// Chat context exports
export {
	ChatProvider,
	useChatContext,
	useBusySessions,
	useChatHistory,
	useSelectedChat,
	type ChatContextValue,
} from "./chat-context";

// Combined session context (composition layer for backward compatibility)
export {
	SessionProvider,
	useSessionContext,
	type SessionContextValue,
} from "./session-context";

export {
	OnboardingProvider,
	useOnboarding,
	useIsUnlocked,
	useNeedsOnboarding,
	type OnboardingState,
	type OnboardingStage,
	type UserLevel,
	type UnlockedComponents,
} from "./onboarding-context";
