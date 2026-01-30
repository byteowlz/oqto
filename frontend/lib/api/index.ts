/**
 * API Client Index
 * Re-exports all API modules for backwards compatibility
 */

// Client infrastructure
export {
	getAuthToken,
	setAuthToken,
	getAuthHeaders,
	authFetch,
	getControlPlaneBaseUrl,
	setControlPlaneBaseUrl,
	controlPlaneDirectBaseUrl,
	controlPlaneApiUrl,
	readApiError,
} from "./client";

// Shared types
export type {
	UserInfo,
	LoginRequest,
	LoginResponse,
	RegisterRequest,
	RegisterResponse,
	WorkspaceSessionStatus,
	WorkspaceMode,
	Persona,
	WorkspaceSession,
	ProjectLogo,
	WorkspaceDirEntry,
	ProjectTemplateEntry,
	ListProjectTemplatesResponse,
	CreateProjectFromTemplateRequest,
	CreateWorkspaceSessionRequest,
	ProjectEntry,
	SessionUpdateInfo,
} from "./types";

// Auth
export {
	login,
	logout,
	register,
	getCurrentUser,
	devLogin,
} from "./auth";

// Sessions
export {
	listWorkspaceSessions,
	createWorkspaceSession,
	getOrCreateWorkspaceSession,
	getOrCreateSessionForWorkspace,
	getWorkspaceSession,
	touchSessionActivity,
	stopWorkspaceSession,
	resumeWorkspaceSession,
	deleteWorkspaceSession,
	restartWorkspaceSession,
	checkSessionUpdate,
	upgradeWorkspaceSession,
} from "./sessions";

// Chat history
export type {
	ChatSession,
	GroupedChatHistory,
	ChatHistoryQuery,
	UpdateChatSessionRequest,
	ChatMessagePart,
	ChatMessage,
} from "./chat";
export {
	listChatHistory,
	listChatHistoryGrouped,
	getChatSession,
	updateChatSession,
	getChatMessages,
	convertChatMessageToOpenCode,
	convertChatMessagesToOpenCode,
} from "./chat";

// Main Chat
export type {
	MainChatHistoryType,
	MainChatHistoryEntry,
	MainChatSession,
	PiSessionFile,
	PiSessionMessage,
	MainChatAssistantInfo,
	MainChatPiStatus,
	PiModelInfo,
	PiPromptCommandInfo,
	PiState,
	PiSessionStats,
	PiAgentMessage,
	PiCompactionResult,
	MainChatDbMessage,
	PiSearchHit,
	PiSearchResponse,
	InSessionSearchResult,
} from "./main-chat";
export {
	listMainChatAssistants,
	getMainChatAssistant,
	createMainChatAssistant,
	updateMainChatAssistant,
	deleteMainChatAssistant,
	getMainChatHistory,
	addMainChatHistory,
	listMainChatSessions,
	listMainChatPiSessions,
	renamePiSession,
	searchMainChatPiSessions,
	newMainChatPiSessionFile,
	getMainChatPiSessionMessages,
	resumeMainChatPiSession,
	searchInPiSession,
	registerMainChatSession,
	getLatestMainChatSession,
	exportMainChatHistory,
	getMainChatPiStatus,
	startMainChatPiSession,
	getMainChatPiState,
	sendMainChatPiPrompt,
	abortMainChatPi,
	getMainChatPiMessages,
	compactMainChatPi,
	setMainChatPiModel,
	getMainChatPiModels,
	getMainChatPiCommands,
	newMainChatPiSession,
	resetMainChatPiSession,
	getMainChatPiStats,
	closeMainChatPiSession,
	createMainChatPiWebSocket,
	getMainChatPiHistory,
	clearMainChatPiHistory,
	addMainChatPiSeparator,
	setWorkspacePiModel,
	getWorkspacePiModels,
	newWorkspacePiSession,
	resumeWorkspacePiSession,
	getWorkspacePiState,
	getWorkspacePiSessionMessages,
	abortWorkspacePiSession,
	createWorkspacePiWebSocket,
} from "./main-chat";

// Projects
export {
	listProjects,
	listWorkspaceDirectories,
	listProjectTemplates,
	createProjectFromTemplate,
	getProjectLogoUrl,
} from "./projects";

// Personas
export {
	listPersonas,
	getPersona,
} from "./personas";

// Features
export type {
	VisualizerVoiceConfig,
	VoiceFeatureConfig,
	SessionAutoAttachMode,
	Features,
} from "./features";
export { getFeatures } from "./features";

// Dashboard
export type {
	SchedulerEntry,
	SchedulerOverview,
	FeedFetchResponse,
	CodexBarUsagePayload,
} from "./dashboard";
export {
	getSchedulerOverview,
	fetchFeed,
	getCodexBarUsage,
} from "./dashboard";

// Files and proxy URLs
export {
	opencodeProxyBaseUrl,
	terminalProxyPath,
	fileserverProxyBaseUrl,
	fileserverWorkspaceBaseUrl,
	mainChatFilesBaseUrl,
	workspaceFileUrl,
	terminalWorkspaceProxyPath,
	memoriesWorkspaceBaseUrl,
	voiceProxyWsUrl,
	browserStreamWsUrl,
} from "./files";

// Config
export type {
	PermissionAction,
	PermissionRule,
	PermissionConfig,
	CompactionConfig,
	ShareMode,
	WorkspaceConfig,
} from "./config";
export {
	getGlobalOpencodeConfig,
	getWorkspaceConfig,
	saveWorkspaceConfig,
} from "./config";

// Settings
export type {
	SettingsValue,
	SettingsValues,
	SettingsUpdateRequest,
} from "./settings";
export {
	getSettingsSchema,
	getSettingsValues,
	updateSettingsValues,
	reloadSettings,
} from "./settings";

// Search
export type {
	HstryAgentFilter,
	HstrySearchQuery,
	HstrySearchHit,
	HstrySearchResponse,
} from "./search";
export { searchSessions } from "./search";

// Agents
export type {
	AgentAskRequest,
	AgentAskResponse,
	AgentAskAmbiguousError,
} from "./agents";
export {
	askAgent,
	AgentAskAmbiguousException,
} from "./agents";

// Onboarding
export type {
	OnboardingStage,
	UserLevel,
	UnlockedComponents,
	OnboardingState,
	UpdateOnboardingRequest,
} from "./onboarding";
export {
	getOnboardingState,
	updateOnboardingState,
	advanceOnboardingStage,
	unlockOnboardingComponent,
	activateOnboardingGodmode,
	completeOnboarding,
	resetOnboarding,
} from "./onboarding";
