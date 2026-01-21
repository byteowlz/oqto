import type { Permission } from "@/lib/opencode-client";

export type SessionErrorInfo = {
	name: string;
	message: string;
};

function extractRecord(value: unknown): Record<string, unknown> | null {
	if (!value || typeof value !== "object") return null;
	const record = value as Record<string, unknown>;
	if (typeof record.properties === "object" && record.properties !== null) {
		return record.properties as Record<string, unknown>;
	}
	return record;
}

export function normalizePermissionEvent(value: unknown): Permission | null {
	const props = extractRecord(value);
	if (!props) return null;

	const id =
		(typeof props.id === "string" && props.id) ||
		(typeof props.permissionID === "string" && props.permissionID) ||
		(typeof props.permissionId === "string" && props.permissionId) ||
		(typeof props.permission_id === "string" && props.permission_id) ||
		"";

	// OpenCode Permission objects use `tool`; Octo WS adapter uses `permission_type`.
	const type =
		(typeof props.type === "string" && props.type) ||
		(typeof (props as { tool?: unknown }).tool === "string" &&
			(props as { tool?: string }).tool) ||
		(typeof props.permissionType === "string" && props.permissionType) ||
		(typeof props.permission_type === "string" && props.permission_type) ||
		"";

	if (!id || !type) return null;

	const title =
		(typeof props.title === "string" && props.title) ||
		(typeof (props as { description?: unknown }).description === "string" &&
			(props as { description?: string }).description) ||
		"";

	// OpenCode Permission objects use `input` (often object) rather than `pattern`.
	const inputOrPattern =
		props.pattern ?? (props as { input?: unknown }).input ?? undefined;

	let pattern: Permission["pattern"] | undefined;
	if (typeof inputOrPattern === "string" || Array.isArray(inputOrPattern)) {
		pattern = inputOrPattern as Permission["pattern"];
	} else if (inputOrPattern && typeof inputOrPattern === "object") {
		const record = inputOrPattern as Record<string, unknown>;
		const displayValue =
			(typeof record.command === "string" && record.command) ||
			(typeof record.cmd === "string" && record.cmd) ||
			(typeof record.path === "string" && record.path) ||
			(typeof record.file === "string" && record.file) ||
			(typeof record.url === "string" && record.url) ||
			(typeof record.directory === "string" && record.directory) ||
			"";
		pattern = displayValue || JSON.stringify(record);
	}

	const metadata: Record<string, unknown> =
		typeof props.metadata === "object" && props.metadata !== null
			? (props.metadata as Record<string, unknown>)
			: {};
	if (
		(props as { risk?: unknown }).risk !== undefined &&
		metadata.risk === undefined
	) {
		metadata.risk = (props as { risk?: unknown }).risk;
	}
	if (
		(props as { input?: unknown }).input !== undefined &&
		metadata.input === undefined
	) {
		metadata.input = (props as { input?: unknown }).input;
	}
	if (
		(props as { description?: unknown }).description !== undefined &&
		metadata.description === undefined
	) {
		metadata.description = (props as { description?: unknown }).description;
	}

	return {
		id,
		type,
		sessionID: typeof props.sessionID === "string" ? props.sessionID : "",
		title,
		pattern,
		metadata,
		time:
			typeof props.time === "object" && props.time !== null
				? (props.time as Permission["time"])
				: { created: Date.now() },
	};
}

export function parseSessionErrorEvent(
	value: unknown,
): SessionErrorInfo | null {
	const props = extractRecord(value);
	if (!props) return null;
	const error =
		props.error && typeof props.error === "object" && props.error !== null
			? (props.error as Record<string, unknown>)
			: null;

	const errorName =
		(typeof props.error_type === "string" && props.error_type) ||
		(typeof props.errorType === "string" && props.errorType) ||
		(typeof props.name === "string" && props.name) ||
		(typeof error?.name === "string" && error.name) ||
		"Error";

	const errorData =
		error?.data && typeof error.data === "object" && error.data !== null
			? (error.data as Record<string, unknown>)
			: null;

	const errorMessage =
		(typeof props.message === "string" && props.message) ||
		(typeof (error as { message?: unknown } | null)?.message === "string" &&
			(error as { message?: string }).message) ||
		(typeof errorData?.message === "string" && errorData.message) ||
		"An unknown error occurred";

	return { name: errorName, message: errorMessage };
}
