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
	ProjectTemplateDefaults,
	ListProjectTemplatesResponse,
	CreateProjectFromTemplateRequest,
	CreateWorkspaceSessionRequest,
	ProjectEntry,
	WorkspaceMeta,
	WorkspaceSandboxConfig,
	WorkspacePiResources,
	WorkspacePiResourcesUpdate,
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
	convertChatMessageToAgent,
	convertChatMessagesToAgent,
} from "./chat";

// Default chat (Pi) APIs
export type {
	PiSessionFile,
	PiSessionMessage,
	AgentState,
	PiModelInfo,
	InSessionSearchResult,
} from "./default-chat";
export {
	setDefaultChatPiModel,
	getDefaultChatPiModels,
	getDefaultChatAgentState,
	startDefaultChatPiSession,
	getDefaultChatAssistant,
	listDefaultChatPiSessions,
	listDefaultChatSessions,
	registerDefaultChatSession,
	searchInPiSession,
	renamePiSession,
} from "./default-chat";

// Projects
export {
	listProjects,
	listWorkspaceDirectories,
	listProjectTemplates,
	createProjectFromTemplate,
	getProjectLogoUrl,
} from "./projects";

export {
	getWorkspaceMeta,
	updateWorkspaceMeta,
	getWorkspaceSandbox,
	updateWorkspaceSandbox,
	getWorkspacePiResources,
	applyWorkspacePiResources,
} from "./workspace";

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
	deleteSchedulerJob,
	fetchFeed,
	getCodexBarUsage,
} from "./dashboard";

// Files and proxy URLs
export {
	agentProxyBaseUrl,
	terminalProxyPath,
	fileserverProxyBaseUrl,
	fileserverWorkspaceBaseUrl,
	defaultChatFilesBaseUrl,
	workspaceFileUrl,
	terminalWorkspaceProxyPath,
	memoriesWorkspaceBaseUrl,
	voiceProxyWsUrl,
	browserStreamWsUrl,
	startBrowser,
	browserAction,
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
	getGlobalAgentConfig,
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
	BootstrapOnboardingRequest,
	BootstrapOnboardingResponse,
} from "./onboarding";
export {
	getOnboardingState,
	updateOnboardingState,
	advanceOnboardingStage,
	unlockOnboardingComponent,
	activateOnboardingGodmode,
	completeOnboarding,
	resetOnboarding,
	bootstrapOnboarding,
} from "./onboarding";
