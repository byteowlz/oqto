import type {
	ChatMessage,
	ModelOption,
	OqtoUiPlatform,
	OqtoUiSnapshot,
	SessionOverview,
	WorkDirectory,
} from "./contracts";

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
	parts?: unknown;
	text?: unknown;
	content?: unknown;
	tool_name?: unknown;
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
		.map(
			(part) => text(part.text) ?? text(part.content) ?? text(part.tool_name),
		)
		.filter((part): part is string => part !== null)
		.join("\n");
}

function parseMessages(value: unknown, sessionId: string): ChatMessage[] {
	return records(value).flatMap((item) => {
		const id = text(item.id);
		if (!id || text(item.session_id) !== sessionId) return [];
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
	async load(requestedSessionId): Promise<OqtoUiSnapshot> {
		const workDirectories = parseWorkDirectories(
			await readJson("/api/chat-history?limit=80"),
		);
		const sessions = workDirectories.flatMap((directory) => directory.sessions);
		const activeSessionId =
			sessions.find((session) => session.id === requestedSessionId)?.id ??
			sessions[0]?.id ??
			null;
		const messages = activeSessionId
			? parseMessages(
					await readJson(
						`/api/chat-history/${encodeURIComponent(activeSessionId)}/messages`,
					),
					activeSessionId,
				)
			: [];
		return {
			workDirectories,
			activeSessionId,
			messages,
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
};
