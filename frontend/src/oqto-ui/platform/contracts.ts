export type SessionSummary = {
	id: string;
	title: string;
	workspace: string;
	updatedAt: number;
	model: string | null;
};

export type TimelineEntry = {
	id: string;
	role: "user" | "assistant" | "tool" | "system";
	text: string;
	status: "committed" | "streaming" | "failed";
};

export type GalleryResource = {
	id: string;
	name: string;
	src: string;
	width: number;
	height: number;
	revision: string;
};

export type OqtoUiSnapshot = {
	sessions: SessionSummary[];
	activeSessionId: string | null;
	timeline: TimelineEntry[];
	gallery: GalleryResource[];
	connection: "connected" | "offline" | "scripted";
};

export type OqtoUiPlatform = {
	load: (sessionId: string | null) => Promise<OqtoUiSnapshot>;
};
