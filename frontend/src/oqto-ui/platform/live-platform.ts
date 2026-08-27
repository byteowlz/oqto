import type {
	ChatMessage,
	MessagePage,
	ModelOption,
	OqtoUiConfigResolution,
	OqtoUiPlatform,
	OqtoUiSnapshot,
	SessionOverview,
	WorkDirectory,
} from "./contracts";
import { DEFAULT_OQTO_UI_CONFIG } from "./contracts";

type JsonRecord = {
	id?: unknown;
	title?: unknown;
	readable_id?: unknown;
	project_name?: unknown;
	workspace_path?: unknown;
	updated_at?: unknown;
	created_at?: unknown;
	model?: unknown;
	session_id?: unknown;
	role?: unknown;
	messages?: unknown;
	has_more?: unknown;
	next_before?: unknown;
	parts?: unknown;
	text?: unknown;
	content?: unknown;
	tool_name?: unknown;
	type?: unknown;
	config?: unknown;
	appearance?: unknown;
	layout?: unknown;
	status_line?: unknown;
	version?: unknown;
	preset?: unknown;
	scheme?: unknown;
	radius?: unknown;
	density?: unknown;
	files?: unknown;
	navigator?: unknown;
	bindings?: unknown;
	segments?: unknown;
	source?: unknown;
	diagnostics?: unknown;
};

function record(value: unknown): JsonRecord | null {
	return typeof value === "object" && value !== null && !Array.isArray(value)
		? (value as JsonRecord)
		: null;
}

function text(value: unknown): string | null {
	return typeof value === "string" ? value : null;
}

function number(value: unknown): number | null {
	return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function records(value: unknown): JsonRecord[] {
	return Array.isArray(value)
		? value.map(record).filter((item): item is JsonRecord => item !== null)
		: [];
}

async function readJson(path: string): Promise<unknown> {
	const response = await fetch(path, {
		credentials: "include",
		headers: { Accept: "application/json" },
	});
	if (!response.ok) throw new Error(`HTTP ${response.status}`);
	return response.json();
}

function two(value: number): string {
	return String(value).padStart(2, "0");
}

function formatTimestamp(value: unknown): string {
	const raw = number(value);
	if (raw === null || raw <= 0) return "";
	const date = new Date(raw < 1_000_000_000_000 ? raw * 1000 : raw);
	return `${date.getFullYear()}/${two(date.getMonth() + 1)}/${two(date.getDate())} - ${two(date.getHours())}:${two(date.getMinutes())}`;
}

function formatTime(value: unknown): string {
	const stamp = formatTimestamp(value);
	return stamp.slice(stamp.indexOf(" - ") + 3);
}

function accentFor(name: string): string {
	return (
		name
			.replace(/[^\p{L}\p{N}]/gu, "")
			.slice(0, 2)
			.toUpperCase() || "??"
	);
}

function parseWorkDirectories(value: unknown): WorkDirectory[] {
	const directories = new Map<string, WorkDirectory>();
	for (const item of records(value)) {
		const id = text(item.id);
		if (!id?.startsWith("oqto-")) continue;
		const path = text(item.workspace_path) ?? "";
		const name =
			text(item.project_name) ?? path.split("/").filter(Boolean).at(-1) ?? "—";
		const key = path || name;
		const session: SessionOverview = {
			id,
			name: text(item.title) ?? text(item.readable_id) ?? id,
			preview: "",
			updated: formatTimestamp(item.updated_at),
			status: "unknown",
			model: text(item.model) ?? "",
		};
		const existing = directories.get(key);
		if (existing) {
			existing.sessions.push(session);
		} else {
			directories.set(key, {
				id: key,
				name,
				path,
				accent: accentFor(name),
				sessions: [session],
			});
		}
	}
	return [...directories.values()];
}

function partText(value: unknown): string {
	return records(value)
		.filter((part) => text(part.type) !== "thinking")
		.map(
			(part) => text(part.text) ?? text(part.content) ?? text(part.tool_name),
		)
		.filter((part): part is string => part !== null)
		.join("\n");
}

function parseMessages(value: unknown): ChatMessage[] {
	return records(value).flatMap((item) => {
		const id = text(item.id);
		if (!id) return [];
		const role = text(item.role);
		const author =
			role === "user" ? "user" : role === "assistant" ? "agent" : "tool";
		if (role !== "user" && role !== "assistant" && role !== "tool") return [];
		return [
			{
				id,
				author,
				content: partText(item.parts),
				time: formatTime(item.created_at),
			} satisfies ChatMessage,
		];
	});
}

const DEFAULT_PAGE_SIZE = 200;
const MAX_PAGE_SIZE = 1_000;

function parseMessagePage(value: unknown, sessionId: string): MessagePage {
	const page = record(value);
	if (!page) {
		return { sessionId, messages: [], hasMore: false, nextBefore: null };
	}
	return {
		sessionId: text(page.session_id) ?? sessionId,
		messages: parseMessages(page.messages),
		hasMore: page.has_more === true,
		nextBefore: text(page.next_before),
	};
}

const invalidConfigResponse = "Invalid config service response";

function parseUiConfig(value: unknown): OqtoUiConfigResolution {
	const root = record(value);
	const config = record(root?.config);
	const appearance = record(config?.appearance);
	const layout = record(config?.layout);
	const statusLine = record(config?.status_line);
	if (
		config?.version !== 1 ||
		typeof config.preset !== "string" ||
		typeof appearance?.scheme !== "string" ||
		typeof appearance.radius !== "string" ||
		typeof appearance.density !== "string" ||
		typeof layout?.files !== "string" ||
		typeof layout.navigator !== "string" ||
		!Array.isArray(config.bindings) ||
		!Array.isArray(statusLine?.segments) ||
		typeof root?.source !== "string" ||
		!Array.isArray(root.diagnostics)
	) {
		return {
			...DEFAULT_OQTO_UI_CONFIG,
			source: "user-lua-fallback",
			diagnostics: [
				{ code: "customization.response.invalid", message: invalidConfigResponse },
			],
		};
	}
	return value as OqtoUiConfigResolution;
}

function modelOptions(directories: WorkDirectory[]): ModelOption[] {
	const ids = new Set<string>();
	for (const directory of directories) {
		for (const session of directory.sessions) {
			if (session.model) ids.add(session.model);
		}
	}
	return [...ids].map((id) => ({ id, name: id }));
}

export const liveOqtoUiPlatform: OqtoUiPlatform = {
	id: "live",
	async loadUiConfig(): Promise<OqtoUiConfigResolution> {
		return parseUiConfig(await readJson("/api/oqto-ui/config"));
	},
	async load(requestedSessionId): Promise<OqtoUiSnapshot> {
		const workDirectories = parseWorkDirectories(
			await readJson("/api/chat-history?limit=80"),
		);
		const sessions = workDirectories.flatMap((directory) => directory.sessions);
		const activeSessionId =
			sessions.find((session) => session.id === requestedSessionId)?.id ??
			sessions[0]?.id ??
			null;
		return {
			workDirectories,
			activeSessionId,
			files: [],
			workArea: {
				tabs: [{ id: "chat", owner: "session" }],
				editorLines: [],
				terminalLines: [],
			},
			gallery: [],
			environment: {
				models: modelOptions(workDirectories),
				statusBar: null,
				connection: "connected",
			},
		};
	},

	async loadMessages(sessionId, before, limit): Promise<MessagePage> {
		const params = new URLSearchParams();
		const size = Math.min(limit ?? DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
		params.set("limit", String(size));
		if (before) params.set("before", before);
		return parseMessagePage(
			await readJson(
				`/api/chat-history/${encodeURIComponent(sessionId)}/messages/page?${params.toString()}`,
			),
			sessionId,
		);
	},
};
