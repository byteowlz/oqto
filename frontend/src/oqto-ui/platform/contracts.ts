import type { JsonValue } from "../engine/projection";
import type { ChatEngineHandle } from "./chat-contract";

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
	/** Committed project logo served by the backend, when one exists. */
	logoUrl?: string;
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

export type ChatMessagePart =
	| { type: "text"; id: string; text: string; format?: "markdown" | "plain" }
	| { type: "thinking"; id: string; text: string }
	| {
			type: "tool_call";
			id: string;
			toolCallId: string;
			name: string;
			input?: JsonValue;
			status: "pending" | "running" | "success" | "error";
	  }
	| {
			type: "tool_result";
			id: string;
			toolCallId: string;
			name?: string;
			output?: JsonValue;
			isError: boolean;
			durationMs?: number;
	  }
	| {
			type: "file_ref";
			id: string;
			uri: string;
			label?: string;
			range?: { startLine?: number; endLine?: number };
	  };

export type ChatMessage = {
	id: string;
	author: "user" | "agent" | "tool";
	content: string;
	parts?: ChatMessagePart[];
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

export type OqtoUiConfig = {
	version: 1;
	preset: string;
	appearance: {
		scheme: "oqto-dark" | "oqto-light" | "nord-dark" | "nord-light";
		radius: "square" | "compact" | "soft";
		density: "compact" | "standard" | "comfortable";
		font: "system" | "mono";
	};
	layout: {
		files: "left" | "right" | "hidden";
		navigator: "left" | "hidden";
	};
	bindings: Array<{ keys: string; action: string }>;
	status_line: {
		segments: Array<
			"session" | "model" | "context" | "connection" | "runnerload" | "version"
		>;
	};
	mobile: {
		mode: "classic" | "corner";
		hold_ms: number;
		corners: {
			top_left: { tap: string; hold: string };
			top_right: { tap: string; hold: string };
			bottom_left: { tap: string; hold: string };
			bottom_right: { tap: string; hold: string };
		};
	};
};

export type OqtoUiConfigResolution = {
	config: OqtoUiConfig;
	source: "dist-default" | "user-lua" | "user-lua-fallback";
	diagnostics: Array<{ code: string; message: string }>;
};

export const DEFAULT_OQTO_UI_CONFIG: OqtoUiConfigResolution = {
	config: {
		version: 1,
		preset: "classic",
		appearance: {
			scheme: "oqto-dark",
			radius: "square",
			density: "standard",
			font: "system",
		},
		layout: { files: "right", navigator: "left" },
		bindings: [
			{ keys: "ctrl+shift+p", action: "shell.openCommandPalette" },
			{ keys: "ctrl+shift+f", action: "view.openFiles" },
		],
		status_line: {
			segments: ["session", "model", "context", "connection"],
		},
		mobile: {
			mode: "classic",
			hold_ms: 320,
			corners: {
				top_left: { tap: "navigator.open", hold: "menu.projects" },
				top_right: { tap: "view.openFiles", hold: "menu.tools" },
				bottom_left: {
					tap: "session.openPrevious",
					hold: "menu.sessionMru",
				},
				bottom_right: { tap: "chat.send", hold: "menu.chatActions" },
			},
		},
	},
	source: "dist-default",
	diagnostics: [],
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
	/** Writable chat engine; scripted adapters provide a deterministic fake. */
	chat: ChatEngineHandle;
	load: (sessionId: string | null) => Promise<OqtoUiSnapshot>;
	loadUiConfig: () => Promise<OqtoUiConfigResolution>;
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
