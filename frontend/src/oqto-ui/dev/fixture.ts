import type { ChatMessage, OqtoUiSnapshot } from "../platform/contracts";

export type ScriptedFixture = Omit<OqtoUiSnapshot, "activeSessionId">;

export const scriptedFixture: ScriptedFixture = {
	workDirectories: [
		{
			id: "oqto",
			name: "oqto_refactor",
			path: "~/byteowlz/oqto_refactor",
			accent: "OQ",
			sessions: [
				{
					id: "frontend-rebuild",
					name: "Frontend shell rebuild",
					preview: "Guardrails are in. Building the approval lab now.",
					updated: "2026/07/30 - 15:02",
					status: "working",
					model: "opus-4.7",
					context: { tokens: "24.1k", percent: 12, window: "200k" },
					tasks: [
						{
							id: "inspect",
							titleKey: "oqtoUi.taskProgress.fixture.inspect",
							status: "completed",
						},
						{
							id: "plan",
							titleKey: "oqtoUi.taskProgress.fixture.plan",
							status: "completed",
						},
						{
							id: "implement",
							titleKey: "oqtoUi.taskProgress.fixture.implement",
							status: "active",
						},
						{
							id: "verify",
							titleKey: "oqtoUi.taskProgress.fixture.verify",
							status: "pending",
						},
						{
							id: "summarize",
							titleKey: "oqtoUi.taskProgress.fixture.summarize",
							status: "pending",
						},
					],
				},
				{
					id: "chat-reliability",
					name: "Chat persistence diagnosis",
					preview: "Four replay cases still need attention.",
					updated: "2026/07/30 - 14:44",
					status: "blocked",
					model: "opus-4.7",
					unread: 2,
					context: { tokens: "61.4k", percent: 31, window: "200k" },
				},
				{
					id: "release-051",
					name: "Prepare v0.5.1",
					preview: "Artifact checks passed on the builder.",
					updated: "2026/07/29 - 18:21",
					status: "done",
					model: "sonnet-4.6",
				},
			],
		},
		{
			id: "skillissues",
			name: "skillissues",
			path: "~/byteowlz/skillissues",
			accent: "SK",
			sessions: [
				{
					id: "skill-audit",
					name: "Audit browser skills",
					preview: "Waiting for the comparison notes.",
					updated: "2026/07/29 - 09:12",
					status: "idle",
					model: "sonnet-4.6",
					context: { tokens: "9.8k", percent: 5, window: "200k" },
				},
			],
		},
		{
			id: "sldr",
			name: "sldr",
			path: "~/byteowlz/sldr",
			accent: "SL",
			sessions: [
				{
					id: "speaker-view",
					name: "Speaker view polish",
					preview: "No runtime connected.",
					updated: "2026/07/26 - 11:03",
					status: "unknown",
					model: "haiku-4.5",
				},
			],
		},
	],
	files: [
		{ id: "src", name: "src", kind: "folder", depth: 0, count: 14 },
		{ id: "oqto-ui", name: "oqto-ui", kind: "folder", depth: 1, count: 4 },
		{
			id: "shell",
			name: "OqtoUiShell.tsx",
			kind: "typescript",
			depth: 2,
			changed: true,
		},
		{
			id: "timeline",
			name: "SessionTimeline.ts",
			kind: "typescript",
			depth: 2,
		},
		{ id: "docs", name: "docs", kind: "folder", depth: 0, count: 31 },
		{ id: "adr", name: "adr", kind: "folder", depth: 1, count: 32 },
		{
			id: "adr31",
			name: "0031-session-centric-workbench-shell.md",
			kind: "markdown",
			depth: 2,
		},
		{ id: "shots", name: "screenshots", kind: "folder", depth: 0, count: 6 },
		{
			id: "desktop",
			name: "oqto-ui-desktop.png",
			kind: "image",
			depth: 1,
		},
		{ id: "agents", name: "AGENTS.md", kind: "markdown", depth: 0 },
		{ id: "cargo", name: "Cargo.toml", kind: "config", depth: 0 },
	],
	workArea: {
		tabs: [
			{ id: "chat", owner: "session" },
			{ id: "editor", owner: "workDirectory", fileName: "OqtoUiShell.tsx" },
			{ id: "terminal", owner: "workDirectory", pinned: true },
			{ id: "gallery", owner: "workDirectory" },
		],
		editorLines: [
			'import { SessionTimeline } from "./SessionTimeline";',
			"",
			"export function OqtoUiShell() {",
			"\tconst timeline = useSessionTimeline();",
			"\treturn (",
			"\t\t<WorkArea>",
			"\t\t\t<ChatTab timeline={timeline} />",
			"\t\t\t<FilesPanel />",
			"\t\t</WorkArea>",
			"\t);",
			"}",
		],
		terminalLines: [
			"$ just check",
			"fmt: ok",
			"lint: ok (0 warnings)",
			"test: 44 passed",
			"$",
		],
	},
	gallery: [
		{
			id: "cover",
			name: "cover.svg",
			src: "/icons/IMAGE_white.svg",
			width: 320,
			height: 240,
			revision: "sha256:7c42a1",
		},
		{
			id: "detail",
			name: "detail.svg",
			src: "/icons/IMAGE_white.svg",
			width: 320,
			height: 240,
			revision: "sha256:7c42a1",
		},
		{
			id: "mobile",
			name: "mobile.svg",
			src: "/icons/IMAGE_white.svg",
			width: 320,
			height: 240,
			revision: "sha256:7c42a1",
		},
		{
			id: "contact-sheet",
			name: "contact-sheet.svg",
			src: "/icons/IMAGE_white.svg",
			width: 320,
			height: 240,
			revision: "sha256:7c42a1",
		},
	],
	environment: {
		models: [
			{ id: "opus-4.7", name: "opus-4.7" },
			{ id: "sonnet-4.6", name: "sonnet-4.6" },
			{ id: "haiku-4.5", name: "haiku-4.5" },
		],
		statusBar: {
			runningSessions: "1",
			onlineUsers: "0/1",
			runnerLoad: "0/26",
			version: "v0.5.0",
		},
		connection: "scripted",
	},
};

/**
 * Deterministic scripted timeline, newest last. Long enough that the
 * development route exercises real pagination and virtualization.
 */
export const scriptedTimeline: ChatMessage[] = (() => {
	const notes = [
		"Inspecting the existing shell composition.",
		"Guardrails hold; extending the scripted adapter.",
		"Timeline pages tile by durable cursor position.",
		"Virtualized rows keep long sessions responsive.",
		"Load-earlier preserves the viewport anchor.",
	];
	return Array.from({ length: 120 }, (_, index) => {
		const minute = 8 + Math.floor(index / 5);
		const author = index % 4 === 0 ? "user" : "agent";
		return {
			id: `scripted-${index + 1}`,
			author,
			content: `[${index + 1}] ${notes[index % notes.length]}`,
			time: `${String(14 + Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`,
		} satisfies ChatMessage;
	});
})();
