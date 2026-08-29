import type { JsonValue } from "../engine/projection";
import { createSessionEngine } from "../engine/session-engine";
import type {
	ChatMessage,
	ChatMessagePart,
	MessagePage,
	ModelOption,
	OqtoUiConfigResolution,
	OqtoUiPlatform,
	OqtoUiSnapshot,
	SessionOverview,
	StatusBarData,
	WorkDirectory,
} from "./contracts";
import { DEFAULT_OQTO_UI_CONFIG } from "./contracts";
import { liveChatTransport } from "./live-chat-transport";

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
	part_type?: unknown;
	text?: unknown;
	content?: unknown;
	tool_name?: unknown;
	tool_call_id?: unknown;
	tool_use_id?: unknown;
	toolCallId?: unknown;
	name?: unknown;
	arguments?: unknown;
	tool_input?: unknown;
	input?: unknown;
	tool_output?: unknown;
	output?: unknown;
	tool_status?: unknown;
	status?: unknown;
	is_error?: unknown;
	isError?: unknown;
	duration_ms?: unknown;
	durationMs?: unknown;
	uri?: unknown;
	label?: unknown;
	range?: unknown;
	startLine?: unknown;
	endLine?: unknown;
	format?: unknown;
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
	font?: unknown;
	files?: unknown;
	navigator?: unknown;
	bindings?: unknown;
	segments?: unknown;
	mobile?: unknown;
	mode?: unknown;
	hold_ms?: unknown;
	corners?: unknown;
	top_left?: unknown;
	top_right?: unknown;
	bottom_left?: unknown;
	bottom_right?: unknown;
	tap?: unknown;
	hold?: unknown;
	source?: unknown;
	diagnostics?: unknown;
	logo?: unknown;
	path?: unknown;
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
			readableId: text(item.readable_id) ?? undefined,
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

function jsonValue(value: unknown): JsonValue | undefined {
	if (
		value === null ||
		typeof value === "string" ||
		typeof value === "boolean"
	) {
		return value;
	}
	if (typeof value === "number" && Number.isFinite(value)) return value;
	if (Array.isArray(value)) {
		const parsed: JsonValue[] = [];
		for (const item of value) {
			const next = jsonValue(item);
			if (next !== undefined) parsed.push(next);
		}
		return parsed;
	}
	if (typeof value !== "object" || value === null) return undefined;
	const parsed: { [key: string]: JsonValue } = {};
	for (const [key, item] of Object.entries(value)) {
		const next = jsonValue(item);
		if (next !== undefined) parsed[key] = next;
	}
	return parsed;
}

function parseMessageParts(
	value: unknown,
	messageId: string,
): ChatMessagePart[] {
	return records(value).flatMap<ChatMessagePart>(
		(part, index): ChatMessagePart[] => {
			// The durable history endpoint exposes ProjectedChatMessagePart, whose
			// wire names intentionally differ from canonical Part. Normalize that
			// protocol DTO here; renderers only receive canonical parts.
			const type = text(part.part_type) ?? text(part.type);
			const id = text(part.id) ?? `${messageId}:part:${index}`;
			if (type === "text") {
				const value = text(part.text) ?? text(part.content);
				if (value === null) return [];
				const format = text(part.format);
				return [
					{
						type: "text" as const,
						id,
						text: value,
						...(format === "plain" || format === "markdown" ? { format } : {}),
					},
				];
			}
			if (type === "thinking") {
				const value = text(part.text);
				return value === null
					? []
					: [{ type: "thinking" as const, id, text: value }];
			}
			const toolCallId =
				text(part.tool_call_id) ??
				text(part.toolCallId) ??
				text(part.tool_use_id);
			if (type === "tool_call" && toolCallId) {
				const rawStatus = text(part.tool_status) ?? text(part.status);
				const status =
					rawStatus === "running" || rawStatus === "in_progress"
						? "running"
						: rawStatus === "success" ||
								rawStatus === "completed" ||
								rawStatus === "done"
							? "success"
							: rawStatus === "error" || rawStatus === "failed"
								? "error"
								: "pending";
				return [
					{
						type: "tool_call" as const,
						id,
						toolCallId,
						name: text(part.tool_name) ?? text(part.name) ?? "tool",
						input: jsonValue(part.tool_input ?? part.input ?? part.arguments),
						status,
					},
				];
			}
			if (type === "tool_result" && toolCallId) {
				const rawStatus = text(part.tool_status) ?? text(part.status);
				return [
					{
						type: "tool_result" as const,
						id,
						toolCallId,
						name: text(part.tool_name) ?? text(part.name) ?? undefined,
						output: jsonValue(
							part.tool_output ?? part.output ?? part.content ?? part.text,
						),
						isError:
							part.is_error === true ||
							part.isError === true ||
							rawStatus === "error" ||
							rawStatus === "failed",
						durationMs:
							number(part.duration_ms) ?? number(part.durationMs) ?? undefined,
					},
				];
			}
			if (type === "file_ref") {
				const uri = text(part.uri);
				if (!uri) return [];
				const range = record(part.range);
				const startLine = number(range?.startLine) ?? undefined;
				const endLine = number(range?.endLine) ?? undefined;
				return [
					{
						type: "file_ref" as const,
						id,
						uri,
						label: text(part.label) ?? undefined,
						...(startLine !== undefined || endLine !== undefined
							? { range: { startLine, endLine } }
							: {}),
					},
				];
			}
			return [];
		},
	);
}

function partsText(parts: ChatMessagePart[]): string {
	return parts
		.filter((part) => part.type === "text")
		.map((part) => part.text)
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
		const parts = parseMessageParts(item.parts, id);
		return [
			{
				id,
				author,
				content: partsText(parts),
				parts,
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
	const mobile = record(config?.mobile);
	const corners = record(mobile?.corners);
	const topLeft = record(corners?.top_left);
	const topRight = record(corners?.top_right);
	const bottomLeft = record(corners?.bottom_left);
	const bottomRight = record(corners?.bottom_right);
	if (
		config?.version !== 1 ||
		typeof config.preset !== "string" ||
		typeof appearance?.scheme !== "string" ||
		typeof appearance.radius !== "string" ||
		typeof appearance.density !== "string" ||
		typeof appearance.font !== "string" ||
		typeof layout?.files !== "string" ||
		typeof layout.navigator !== "string" ||
		!Array.isArray(config.bindings) ||
		!Array.isArray(statusLine?.segments) ||
		typeof mobile?.mode !== "string" ||
		typeof mobile.hold_ms !== "number" ||
		!topLeft ||
		!topRight ||
		!bottomLeft ||
		!bottomRight ||
		typeof root?.source !== "string" ||
		!Array.isArray(root.diagnostics)
	) {
		return {
			...DEFAULT_OQTO_UI_CONFIG,
			source: "user-lua-fallback",
			diagnostics: [
				{
					code: "customization.response.invalid",
					message: invalidConfigResponse,
				},
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

/**
 * Attach committed project logos. `/api/projects` reports each directory's
 * absolute path plus its discovered logo (relative to the project root); the
 * logo endpoint serves it under the workspace-relative project path, which
 * for root-level projects is the path's last segment.
 */
async function attachProjectLogos(
	directories: WorkDirectory[],
): Promise<WorkDirectory[]> {
	try {
		const projects = records(await readJson("/api/projects"));
		const logosByPath = new Map<string, string>();
		for (const project of projects) {
			const absolutePath = text(project.path);
			const logo = record(project.logo);
			const logoPath = text(logo?.path);
			if (!absolutePath || !logoPath) continue;
			const relativeProjectPath = absolutePath
				.split("/")
				.filter(Boolean)
				.at(-1);
			if (!relativeProjectPath) continue;
			logosByPath.set(
				absolutePath,
				`/api/projects/logo/${encodeURIComponent(relativeProjectPath)}/${logoPath
					.split("/")
					.map(encodeURIComponent)
					.join("/")}`,
			);
		}
		return directories.map((directory) => {
			const logoUrl = logosByPath.get(directory.path);
			return logoUrl ? { ...directory, logoUrl } : directory;
		});
	} catch {
		// Logos are decoration; a failed lookup keeps the procedural fallback.
		return directories;
	}
}

/// Status strip data: version from the public health endpoint, admin
/// counters only when the caller is an admin (403 otherwise is normal).
async function loadStatusBar(): Promise<StatusBarData | null> {
	let version = "";
	try {
		const health = (await readJson("/api/health")) as { version?: unknown };
		const raw = text(health.version);
		if (raw) version = `v${raw.replace(/^v/, "")}`;
	} catch {
		return null;
	}
	let onlineUsers = "";
	let runnerLoad = "";
	try {
		const stats = (await readJson("/api/admin/stats")) as {
			active_users?: unknown;
			total_users?: unknown;
			running_sessions?: unknown;
			total_sessions?: unknown;
		};
		const count = (value: unknown) => {
			const parsed = Number(value);
			return Number.isFinite(parsed) ? String(parsed) : "?";
		};
		onlineUsers = `${count(stats.active_users)}/${count(stats.total_users)}`;
		runnerLoad = `${count(stats.running_sessions)}/${count(stats.total_sessions)}`;
	} catch {
		// Non-admin callers cannot read counters; the rest of the strip stays.
	}
	return { runningSessions: "", onlineUsers, runnerLoad, version };
}

export const liveOqtoUiPlatform: OqtoUiPlatform = {
	id: "live",
	chat: createSessionEngine(liveChatTransport),
	async loadUiConfig(): Promise<OqtoUiConfigResolution> {
		return parseUiConfig(await readJson("/api/oqto-ui/config"));
	},
	async load(requestedSessionId): Promise<OqtoUiSnapshot> {
		const workDirectories = await attachProjectLogos(
			parseWorkDirectories(await readJson("/api/chat-history?limit=80")),
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
				statusBar: await loadStatusBar(),
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
