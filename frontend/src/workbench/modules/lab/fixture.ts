import type { WorkbenchLabFixture } from "./model";

export const workbenchLabFixture: WorkbenchLabFixture = {
	workspaceName: "byteowlz",
	workDirectories: [
		{
			id: "oqto",
			name: "oqto_refactor",
			path: "~/byteowlz/oqto_refactor",
			accent: "OQ",
			sessions: [
				{
					id: "frontend-rebuild",
					name: "Frontend workbench rebuild",
					preview: "Guardrails are in. Building the approval lab now.",
					updated: "2026/07/30 - 15:02",
					status: "working",
					model: "opus-4.7",
					context: { tokens: "24.1k", percent: 12, window: "200k" },
					tasks: [
						{
							id: "inspect",
							titleKey: "workbench.taskProgress.fixture.inspect",
							status: "completed",
						},
						{
							id: "plan",
							titleKey: "workbench.taskProgress.fixture.plan",
							status: "completed",
						},
						{
							id: "implement",
							titleKey: "workbench.taskProgress.fixture.implement",
							status: "active",
						},
						{
							id: "verify",
							titleKey: "workbench.taskProgress.fixture.verify",
							status: "pending",
						},
						{
							id: "summarize",
							titleKey: "workbench.taskProgress.fixture.summarize",
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
		{ id: "workbench", name: "workbench", kind: "folder", depth: 1, count: 4 },
		{
			id: "shell",
			name: "WorkbenchShell.tsx",
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
			name: "workbench-desktop.png",
			kind: "image",
			depth: 1,
		},
		{ id: "agents", name: "AGENTS.md", kind: "markdown", depth: 0 },
		{ id: "cargo", name: "Cargo.toml", kind: "config", depth: 0 },
	],
	messages: [
		{
			id: "m1",
			author: "user",
			content:
				"Update the workbench shell so files stay visible while I switch sessions.",
			time: "14:59",
		},
		{
			id: "m2",
			author: "agent",
			content:
				"I’ll keep the existing Oqto interaction patterns and make session scope explicit without moving the shared file context.",
			time: "15:00",
			activity: {
				kind: "read",
				name: "docs/adr/0031-session-centric-workbench-shell.md",
				state: "done",
			},
		},
		{
			id: "m3",
			author: "agent",
			content:
				"The shared file tree stays mounted next to the conversation, so switching sessions only swaps the timeline.",
			time: "15:02",
			activity: {
				kind: "edit",
				name: "src/workbench/WorkbenchShell.tsx",
				state: "active",
			},
		},
	],
	workArea: {
		tabs: [
			{ id: "chat", owner: "session" },
			{ id: "editor", owner: "workDirectory", fileName: "WorkbenchShell.tsx" },
			{ id: "terminal", owner: "workDirectory", pinned: true },
		],
		editorLines: [
			'import { SessionTimeline } from "./SessionTimeline";',
			"",
			"export function WorkbenchShell() {",
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
};
