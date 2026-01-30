/**
 * Settings API
 * Schema, values, and updates for app settings
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";

// ============================================================================
// Settings Types
// ============================================================================

/** A settings value with metadata */
export type SettingsValue = {
	/** The current value */
	value: unknown;
	/** Whether this value is explicitly set in config (vs default) */
	is_configured: boolean;
	/** The default value from schema (if any) */
	default?: unknown;
};

/** Map of dotted paths to settings values */
export type SettingsValues = Record<string, SettingsValue>;

/** Request to update settings */
export type SettingsUpdateRequest = {
	values: Record<string, unknown>;
};

// ============================================================================
// Settings API
// ============================================================================

function buildSettingsQuery(app: string, workspacePath?: string): string {
	const params = new URLSearchParams({ app });
	if (workspacePath) params.set("workspace_path", workspacePath);
	return params.toString();
}

/** Get the JSON schema for an app's settings (filtered by user permissions) */
export async function getSettingsSchema(
	app: string,
	workspacePath?: string,
): Promise<unknown> {
	const res = await authFetch(
		controlPlaneApiUrl(
			`/api/settings/schema?${buildSettingsQuery(app, workspacePath)}`,
		),
		{
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get current settings values for an app */
export async function getSettingsValues(
	app: string,
	workspacePath?: string,
): Promise<SettingsValues> {
	const res = await authFetch(
		controlPlaneApiUrl(
			`/api/settings?${buildSettingsQuery(app, workspacePath)}`,
		),
		{
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Update settings values for an app */
export async function updateSettingsValues(
	app: string,
	updates: SettingsUpdateRequest,
	workspacePath?: string,
): Promise<SettingsValues> {
	const res = await authFetch(
		controlPlaneApiUrl(
			`/api/settings?${buildSettingsQuery(app, workspacePath)}`,
		),
		{
			method: "PATCH",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(updates),
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Reload settings from disk (admin only) */
export async function reloadSettings(
	app: string,
	workspacePath?: string,
): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(
			`/api/settings/reload?${buildSettingsQuery(app, workspacePath)}`,
		),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
}
