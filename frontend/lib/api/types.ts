/**
 * Shared types used across API modules
 */

// ============================================================================
// Auth Types
// ============================================================================

export type UserInfo = {
	id: string;
	name: string;
	email: string;
	role: string;
};

export type LoginRequest = {
	username: string;
	password: string;
};

export type LoginResponse = {
	token: string;
	user: UserInfo;
};

export type RegisterRequest = {
	username: string;
	email: string;
	password: string;
	invite_code: string;
	display_name?: string;
};

export type RegisterResponse = {
	token: string;
	user: UserInfo;
};

// ============================================================================
// Session Types
// ============================================================================

export type WorkspaceSessionStatus =
	| "pending"
	| "starting"
	| "running"
	| "stopping"
	| "stopped"
	| "failed";

/** Workspace mode for a persona */
export type WorkspaceMode = "default_only" | "ask" | "any";

/** Persona metadata from persona.toml */
export type Persona = {
	/** Unique identifier (directory name) */
	id: string;
	/** Display name of the persona */
	name: string;
	/** Short description of what this persona does */
	description: string;
	/** Accent color for UI (hex color, e.g., "#6366f1") */
	color?: string | null;
	/** Path to avatar image (relative to persona directory) */
	avatar?: string | null;
	/** Whether this is the default persona */
	is_default: boolean;
	/** agent ID to use */
	agent_id: string;
	/** Default working directory (optional) */
	default_workdir?: string | null;
	/** Workspace mode (default_only, ask, or any) */
	workspace_mode: WorkspaceMode;
	/** If true, has own directory with agent config. If false, wrapper for existing agent */
	standalone: boolean;
	/** If true, can work on external projects. If false, only works in own persona directory */
	project_access: boolean;
};

export type WorkspaceSession = {
	id: string;
	container_id: string | null;
	container_name: string;
	user_id: string;
	workspace_path: string;
	persona_path: string | null;
	image: string;
	agent_port: number;
	fileserver_port: number;
	ttyd_port: number;
	status: WorkspaceSessionStatus;
	created_at: string;
	started_at: string | null;
	stopped_at: string | null;
	error_message: string | null;
	/** Persona metadata (if session has a persona_path with persona.toml) */
	persona?: Persona | null;
};

export type ProjectLogo = {
	/** Path relative to project root (e.g., "logo/project_logo_white.svg") */
	path: string;
	/** Logo variant (e.g., "white", "black", "white_on_black") */
	variant: string;
};

export type WorkspaceDirEntry = {
	name: string;
	path: string;
	type: "directory";
	/** Project logo if found in logo/ directory */
	logo?: ProjectLogo;
};

export type ProjectTemplateDefaults = {
	display_name?: string | null;
	sandbox_profile?: string | null;
	default_provider?: string | null;
	default_model?: string | null;
	skills_mode?: "all" | "custom";
	extensions_mode?: "all" | "custom";
	skills?: string[];
	extensions?: string[];
};

export type ProjectTemplateEntry = {
	name: string;
	path: string;
	description?: string;
	defaults?: ProjectTemplateDefaults;
};

export type ListProjectTemplatesResponse = {
	/** Whether templates are configured (repo_path is set). */
	configured: boolean;
	/** List of available templates. */
	templates: ProjectTemplateEntry[];
};

export type CreateProjectFromTemplateRequest = {
	template_path: string;
	project_path: string;
	shared?: boolean;
};

export type CreateWorkspaceSessionRequest = {
	workspace_path?: string;
	image?: string;
	/** Persona ID to use for this session */
	persona_id?: string;
	env?: Record<string, string>;
};

/** Project/workspace directory entry */
export type ProjectEntry = {
	name: string;
	path: string;
	type: "directory";
	/** Project logo if found in logo/ directory */
	logo?: ProjectLogo;
};

export type WorkspaceMeta = {
	display_name?: string | null;
};

export type WorkspaceSandboxConfig = {
	enabled: boolean;
	profile: string;
	profiles: string[];
};

export type WorkspacePiResourceEntry = {
	name: string;
	selected: boolean;
	/** Platform-default resources that cannot be deactivated. */
	mandatory?: boolean;
};

export type WorkspacePiResources = {
	skills_mode: "all" | "custom";
	extensions_mode: "all" | "custom";
	skills: WorkspacePiResourceEntry[];
	extensions: WorkspacePiResourceEntry[];
	global_skills_dir: string;
	global_extensions_dir: string;
};

export type WorkspacePiResourcesUpdate = {
	workspace_path: string;
	skills_mode: "all" | "custom";
	extensions_mode: "all" | "custom";
	skills: string[];
	extensions: string[];
};

export type SessionUpdateInfo = {
	update_available: boolean;
	current_digest: string | null;
	latest_digest: string | null;
};
