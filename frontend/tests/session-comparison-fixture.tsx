// Design-only adapter. Actual OqtoUI shell/components, not the default legacy route.
// Built with an immutable shared fixture supplied by the comparison builder.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { createRoot } from "react-dom/client";
import { initI18n } from "../lib/i18n";
import { OqtoUiShell } from "../src/oqto-ui/app/OqtoUiShell";
import { scriptedOqtoUiPlatform } from "../src/oqto-ui/dev/scripted-platform";
import {
	type ChatMessage,
	DEFAULT_OQTO_UI_CONFIG,
	type OqtoUiPlatform,
	type OqtoUiSnapshot,
} from "../src/oqto-ui/platform/contracts";
import "../src/styles/globals.css";

type ComparisonFixture = {
	sessionId: string;
	sessionName: string;
	workDirectory: string;
	workDirectoryName: string;
	model: string;
	contextPercent: number;
	createdAtMs: number;
	draft: string;
	messages: Array<{
		id: string;
		role: "user" | "assistant";
		parts: NonNullable<ChatMessage["parts"]>;
	}>;
};
declare const __SESSION_COMPARISON_FIXTURE__: ComparisonFixture;
const fixture = __SESSION_COMPARISON_FIXTURE__;
const params = new URLSearchParams(location.search);
const variant = params.get("variant") ?? "incumbent";
const state = params.get("state") ?? "ready";
const rich = params.get("scenario") === "media";
const scheme =
	params.get("scheme") === "oqto-dark" ? "oqto-dark" : "oqto-light";
const previewCode =
	"export function fresh(age: number, ttl: number): boolean {\n  // Expired entries must not survive the boundary.\n  return age < ttl;\n}\n";
const previewMarkdown =
	"# Cache boundary\n\nThe **public API** is unchanged.\n\n| Case | Result |\n| --- | --- |\n| age < ttl | fresh |\n| age = ttl | expired |\n";
const richMessages: ChatMessage[] = rich
	? [
			{
				id: "synthetic-media-turn",
				author: "agent",
				content: "",
				time: "09:42",
				parts: [
					{
						type: "thinking",
						id: "synthetic-thought",
						text: "Checking the preview without losing its full composition.",
					},
					{
						type: "tool_call",
						id: "synthetic-call-part",
						toolCallId: "synthetic-failed-call",
						name: "bash",
						input: { command: "just test expired-entry" },
						status: "error",
					},
					{
						type: "tool_result",
						id: "synthetic-result-part",
						toolCallId: "synthetic-failed-call",
						name: "bash",
						output: "Synthetic failure: expired entry was retained.",
						isError: true,
					},
					{
						type: "image",
						id: "synthetic-image",
						source: "url",
						url: new URL("./fixtures/compact-image.svg", import.meta.url).href,
						alt: "Synthetic full composition",
					},
					{
						type: "text",
						id: "synthetic-caption",
						text: "The whole image stays in flow. Inspect the code and Markdown in Files without losing this draft.",
					},
				],
			},
		]
	: [];
const platformId = "studio-session-comparison";

// No API requests from a design preview can escape to a real control plane.
const assetFetch = window.fetch.bind(window);
window.fetch = (input, init) => {
	const url = new URL(
		input instanceof Request ? input.url : String(input),
		location.href,
	);
	return url.pathname.startsWith("/api/")
		? Promise.resolve(Response.json({ voice: null }))
		: assetFetch(input, init);
};

const snapshot: OqtoUiSnapshot = {
	workDirectories: [
		{
			id: "fixture-directory",
			name: fixture.workDirectoryName,
			path: fixture.workDirectory,
			accent: "CL",
			sessions: [
				{
					id: fixture.sessionId,
					name: fixture.sessionName,
					preview: "TTL boundary and expiry tests",
					updated: "2026/11/19 - 09:41",
					status: state === "working" ? "working" : "idle",
					model: fixture.model,
					context: {
						tokens: "36k",
						percent: fixture.contextPercent,
						window: "200k",
					},
				},
			],
		},
	],
	activeSessionId: fixture.sessionId,
	files: [],
	gallery: [],
	workArea: {
		tabs: [{ id: "chat", owner: "session" }],
		editorLines: [],
		terminalLines: [],
	},
	environment: {
		models: [{ id: fixture.model, name: fixture.model }],
		statusBar: null,
		connection: "scripted",
	},
};
const platform: OqtoUiPlatform = {
	...scriptedOqtoUiPlatform,
	id: platformId,
	files: rich
		? {
				...scriptedOqtoUiPlatform.files,
				list: async () =>
					["cache.ts", "NOTES.md", "image.svg"].map((name) => ({
						path: name,
						name,
						directory: false,
						symlink: false,
						size: 240,
						modifiedAt: fixture.createdAtMs,
					})),
				read: async (_workspace, path) =>
					path.endsWith(".md") ? previewMarkdown : previewCode,
			}
		: scriptedOqtoUiPlatform.files,
	chat: {
		bind: (_session, update) => {
			if (state === "working")
				queueMicrotask(() => update({ kind: "draft", draft: { parts: [] } }));
			return () => {};
		},
		send: () => "fixture-local-preview-only",
		abort: () => {},
	},
	load: async () => snapshot,
	loadMessages: async (sessionId) => ({
		sessionId,
		messages: [
			...fixture.messages.map(
				(message): ChatMessage => ({
					id: message.id,
					author: message.role === "user" ? "user" : "agent",
					content: "",
					parts: message.parts,
					time: new Date(fixture.createdAtMs).toLocaleTimeString("en-GB", {
						hour: "2-digit",
						minute: "2-digit",
					}),
				}),
			),
			...richMessages,
		],
		hasMore: false,
		nextBefore: null,
	}),
	loadUiConfig: async () => ({
		...DEFAULT_OQTO_UI_CONFIG,
		config: {
			...DEFAULT_OQTO_UI_CONFIG.config,
			appearance: {
				...DEFAULT_OQTO_UI_CONFIG.config.appearance,
				scheme,
			},
			layout: {
				files: variant === "focus" ? "hidden" : "right",
				navigator: variant === "focus" ? "hidden" : "left",
			},
		},
	}),
};

const scope = JSON.stringify([
	platformId,
	fixture.workDirectory,
	fixture.sessionId,
]);
localStorage.setItem(`oqto-ui:chat-draft:${scope}`, fixture.draft);
localStorage.setItem(
	"oqto:chatVerbosity",
	variant === "inspect" ? "3" : variant === "focus" ? "1" : "2",
);
initI18n();
const client = new QueryClient({
	defaultOptions: { queries: { retry: false } },
});
function Surface() {
	const [navigation, setNavigation] = useState({
		sessionId: fixture.sessionId,
		workAreaTab: "chat",
		mobileView: "chat",
	});
	return (
		<QueryClientProvider client={client}>
			<OqtoUiShell
				platform={platform}
				workDirectoryId="fixture-directory"
				sessionId={navigation.sessionId}
				mobileView={navigation.mobileView}
				workAreaTab={navigation.workAreaTab}
				schemeId={scheme}
				onNavigate={(next) =>
					setNavigation((current) => ({ ...current, ...next }))
				}
			/>
		</QueryClientProvider>
	);
}
const root = document.getElementById("root");
if (!root) throw new Error("Missing comparison root");
createRoot(root).render(<Surface />);
