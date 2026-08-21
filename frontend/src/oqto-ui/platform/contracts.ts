export type SessionStatus = "working" | "blocked" | "done" | "idle" | "unknown";

export type SessionTask = {
	id: string;
	titleKey: string;
	status: "completed" | "active" | "pending";
};

export type SessionOverview = {
	id: string;
	name: string;
	preview: string;
	updated: string;
	status: SessionStatus;
	model: string;
	tasks?: SessionTask[];
	unread?: number;
	context?: { tokens: string; percent: number; window: string };
};

export type ModelOption = {
	id: string;
	name: string;
};

export type WorkDirectory = {
	id: string;
	name: string;
	path: string;
	accent: string;
	sessions: SessionOverview[];
};

export type FileNode = {
	id: string;
	name: string;
	kind: "folder" | "typescript" | "markdown" | "image" | "config";
	depth: number;
	count?: number;
	changed?: boolean;
};

export type ChatMessage = {
	id: string;
	author: "user" | "agent" | "tool";
	content: string;
	time: string;
	activity?: {
		kind: "read" | "edit" | "test";
		name: string;
		state: "done" | "active";
	};
};

export type WorkAreaTabId = "chat" | "editor" | "terminal" | "gallery";

export type WorkAreaTab = {
	id: WorkAreaTabId;
	owner: "session" | "workDirectory";
	fileName?: string;
	pinned?: boolean;
};

export type StatusBarData = {
	runningSessions: string;
	onlineUsers: string;
	runnerLoad: string;
	version: string;
};

export type WorkArea = {
	tabs: WorkAreaTab[];
	editorLines: string[];
	terminalLines: string[];
};

export type GalleryResource = {
	id: string;
	name: string;
	src: string;
	width: number;
	height: number;
	revision: string;
};

export type EnvironmentInfo = {
	models: ModelOption[];
	statusBar: StatusBarData | null;
	connection: "connected" | "offline" | "scripted";
};

export type OqtoUiSnapshot = {
	workDirectories: WorkDirectory[];
	activeSessionId: string | null;
	files: FileNode[];
	workArea: WorkArea;
	gallery: GalleryResource[];
	environment: EnvironmentInfo;
};

/** One page of timeline messages, oldest-first, ending at `nextBefore`. */
export type MessagePage = {
	sessionId: string;
	messages: ChatMessage[];
	hasMore: boolean;
	/** Opaque cursor selecting messages strictly older than this page. */
	nextBefore: string | null;
};

export type OqtoUiPlatform = {
	/** Stable adapter identity, part of every query key. */
	readonly id: string;
	load: (sessionId: string | null) => Promise<OqtoUiSnapshot>;
	/**
	 * Load one page of a Session's timeline, oldest-first. `before` is an
	 * opaque cursor from a previous page's `nextBefore`; omitted returns the
	 * newest page.
	 */
	loadMessages: (
		sessionId: string,
		before?: string,
		limit?: number,
	) => Promise<MessagePage>;
};

export type UiNavigation = {
	workDirectoryId?: string;
	sessionId?: string;
	mobileView?: "chat" | "files";
	schemeId?: string;
	workAreaTab?: WorkAreaTabId;
};
