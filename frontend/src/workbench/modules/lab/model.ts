export type WorkbenchStatus =
	| "working"
	| "blocked"
	| "done"
	| "idle"
	| "unknown";

export type LabTask = {
	id: string;
	titleKey: string;
	status: "completed" | "active" | "pending";
};

export type LabSession = {
	id: string;
	name: string;
	preview: string;
	updated: string;
	status: WorkbenchStatus;
	model: string;
	tasks?: LabTask[];
	unread?: number;
	context?: { tokens: string; percent: number; window: string };
};

export type LabModelOption = {
	id: string;
	name: string;
};

export type LabWorkDirectory = {
	id: string;
	name: string;
	path: string;
	accent: string;
	sessions: LabSession[];
};

export type LabFile = {
	id: string;
	name: string;
	kind: "folder" | "typescript" | "markdown" | "image" | "config";
	depth: number;
	count?: number;
	changed?: boolean;
};

export type LabMessage = {
	id: string;
	author: "user" | "agent";
	content: string;
	time: string;
	activity?: {
		kind: "read" | "edit" | "test";
		name: string;
		state: "done" | "active";
	};
};

export type LabWorkAreaTab = {
	id: "chat" | "editor" | "terminal";
	owner: "session" | "workDirectory";
	fileName?: string;
	pinned?: boolean;
};

export type LabStatusBar = {
	runningSessions: string;
	onlineUsers: string;
	runnerLoad: string;
	version: string;
};

export type LabWorkArea = {
	tabs: LabWorkAreaTab[];
	editorLines: string[];
	terminalLines: string[];
};

export type WorkbenchLabFixture = {
	workspaceName: string;
	workDirectories: LabWorkDirectory[];
	files: LabFile[];
	messages: LabMessage[];
	workArea: LabWorkArea;
	models: LabModelOption[];
	statusBar: LabStatusBar;
};

export type LabNavigation = {
	workDirectoryId?: string;
	sessionId?: string;
	mobileView?: "chat" | "files";
	schemeId?: string;
	workAreaTab?: "chat" | "editor" | "terminal";
};
