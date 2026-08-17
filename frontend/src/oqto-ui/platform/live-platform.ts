import type {
	GalleryResource,
	OqtoUiPlatform,
	OqtoUiSnapshot,
	SessionSummary,
	TimelineEntry,
} from "./contracts";

type JsonRecord = {
	id?: unknown;
	title?: unknown;
	readable_id?: unknown;
	project_name?: unknown;
	workspace_path?: unknown;
	updated_at?: unknown;
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

function parseSessions(value: unknown): SessionSummary[] {
	return records(value).flatMap((item) => {
		const id = text(item.id);
		if (!id?.startsWith("oqto-")) return [];
		return [
			{
				id,
				title: text(item.title) ?? text(item.readable_id) ?? id,
				workspace: text(item.project_name) ?? text(item.workspace_path) ?? "—",
				updatedAt: number(item.updated_at) ?? 0,
				model: text(item.model),
			},
		];
	});
}

function partText(value: unknown): string {
	return records(value)
		.map(
			(part) => text(part.text) ?? text(part.content) ?? text(part.tool_name),
		)
		.filter((value): value is string => value !== null)
		.join("\n");
}

function parseTimeline(value: unknown, sessionId: string): TimelineEntry[] {
	return records(value).flatMap((item) => {
		const id = text(item.id);
		if (!id || text(item.session_id) !== sessionId) return [];
		const role = text(item.role);
		if (
			role !== "user" &&
			role !== "assistant" &&
			role !== "tool" &&
			role !== "system"
		) {
			return [];
		}
		return [
			{
				id,
				role,
				text: partText(item.parts),
				status: "committed" as const,
			},
		];
	});
}

function galleryResources(): GalleryResource[] {
	return [];
}

export const liveOqtoUiPlatform: OqtoUiPlatform = {
	async load(requestedSessionId) {
		const sessions = parseSessions(
			await readJson("/api/chat-history?limit=80"),
		);
		const activeSessionId =
			sessions.find((session) => session.id === requestedSessionId)?.id ??
			sessions[0]?.id ??
			null;
		const timeline = activeSessionId
			? parseTimeline(
					await readJson(
						`/api/chat-history/${encodeURIComponent(activeSessionId)}/messages`,
					),
					activeSessionId,
				)
			: [];
		return {
			sessions,
			activeSessionId,
			timeline,
			gallery: galleryResources(),
			connection: "connected",
		} satisfies OqtoUiSnapshot;
	},
};
