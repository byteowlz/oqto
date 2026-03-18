// Dialogs
export {
	DeleteConfirmDialog,
	NewProjectDialog,
	RenameProjectDialog,
	RenameSessionDialog,
} from "./dialogs";
export type {
	DeleteConfirmDialogProps,
	NewProjectDialogProps,
	RenameProjectDialogProps,
	RenameSessionDialogProps,
} from "./dialogs";

// Hooks
export {
	useAppShellBootstrap,
	useAppShellProjectEvents,
	useAppShellRouteSync,
	useAppShellSessionAutomation,
	useAppShellSettings,
	useBranchGraphShortcut,
	useGodmodeShortcut,
	useProjectActions,
	useSessionData,
	useSessionDialogs,
	useShellLoadingState,
	useSidebarState,
} from "./hooks";
export type {
	ProjectActionsState,
	ProjectSummary,
	SessionDataInput,
	SessionDataOutput,
	SessionDialogsState,
	SidebarState,
	WorkspaceDirectory,
} from "./hooks";

// Components
export { MobileHeader } from "./MobileHeader";
export type { MobileHeaderProps } from "./MobileHeader";

export { MobileMenu } from "./MobileMenu";
export type { MobileMenuProps, ProjectSummary } from "./MobileMenu";

export { SidebarNav } from "./SidebarNav";
export type { SidebarNavProps } from "./SidebarNav";

export { SidebarSessions } from "./SidebarSessions";
export type {
	SessionsByProject,
	SessionHierarchy,
	SidebarSessionsProps,
} from "./SidebarSessions";

export { SidebarSharedWorkspaces } from "./SidebarSharedWorkspaces";
export type { SidebarSharedWorkspacesProps } from "./SidebarSharedWorkspaces";

export { WorkspaceIcon } from "./WorkspaceIcon";
export type { WorkspaceIconProps } from "./WorkspaceIcon";
