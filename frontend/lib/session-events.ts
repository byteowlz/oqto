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
	const type =
		(typeof props.type === "string" && props.type) ||
		(typeof props.permissionType === "string" && props.permissionType) ||
		(typeof props.permission_type === "string" && props.permission_type) ||
		"";
	if (!id || !type) return null;
	return {
		id,
		type,
		sessionID: typeof props.sessionID === "string" ? props.sessionID : "",
		title: typeof props.title === "string" ? props.title : "",
		pattern:
			typeof props.pattern === "string" || Array.isArray(props.pattern)
				? (props.pattern as Permission["pattern"])
				: undefined,
		metadata:
			typeof props.metadata === "object" && props.metadata !== null
				? (props.metadata as Record<string, unknown>)
				: {},
		time:
			typeof props.time === "object" && props.time !== null
				? (props.time as Permission["time"])
				: { created: Date.now() },
	};
}

export function parseSessionErrorEvent(value: unknown): SessionErrorInfo | null {
	const props = extractRecord(value);
	if (!props) return null;
	const error =
		props.error && typeof props.error === "object" && props.error !== null
			? (props.error as Record<string, unknown>)
			: null;
	const errorName =
		(typeof props.error_type === "string" && props.error_type) ||
		(typeof props.errorType === "string" && props.errorType) ||
		(typeof error?.name === "string" && error.name) ||
		"Error";
	const errorData =
		error?.data && typeof error.data === "object" && error.data !== null
			? (error.data as Record<string, unknown>)
			: null;
	const errorMessage =
		(typeof props.message === "string" && props.message) ||
		(typeof errorData?.message === "string" && errorData.message) ||
		"An unknown error occurred";
	return { name: errorName, message: errorMessage };
}
