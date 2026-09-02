import {
	type ContentRef,
	type LayoutSnapshot,
	contentIdFrom,
	createClassicPresetLayout,
} from "@/src/oqto-ui/compositor/index";

export const sessionsContent: ContentRef = {
	id: contentIdFrom("sessions:catalog"),
	kind: "sessions",
};
export const chatContent: ContentRef = {
	id: contentIdFrom("chat:session-1"),
	kind: "chat",
};
export const filesContent: ContentRef = {
	id: contentIdFrom("files:workdir-1"),
	kind: "files",
};
export const gitContent: ContentRef = {
	id: contentIdFrom("git:workdir-1"),
	kind: "git",
};
export const terminalContent: ContentRef = {
	id: contentIdFrom("terminal:workdir-1"),
	kind: "terminal",
};

export const VIEWPORT = { inlineSize: 1600, blockSize: 900 };

/** arrangement-0; nav=container-5, primary=container-6, auxiliary=container-7. */
export function classicLayout(): LayoutSnapshot {
	return createClassicPresetLayout({
		navigation: [sessionsContent],
		primary: [chatContent],
		auxiliary: [filesContent],
	});
}

export function containerByRole(snapshot: LayoutSnapshot, role: string) {
	const container = snapshot.containers.find(
		(candidate) => candidate.role === role,
	);
	if (!container) throw new Error(`missing container with role ${role}`);
	return container;
}

export function activeArrangementOf(snapshot: LayoutSnapshot) {
	const arrangement = snapshot.arrangements.find(
		(candidate) => candidate.id === snapshot.activeArrangementId,
	);
	if (!arrangement) throw new Error("no active arrangement");
	return arrangement;
}

export const LABELS = {
	closeTab: "close-tab",
	resizeColumns: "resize-columns",
	resizeRows: "resize-rows",
	dropTop: "drop-top",
	dropBottom: "drop-bottom",
	dropStart: "drop-start",
	dropEnd: "drop-end",
};
