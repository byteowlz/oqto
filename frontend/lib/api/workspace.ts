/**
 * Workspace overview API
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";
import type {
	WorkspaceMeta,
	WorkspaceSandboxConfig,
	WorkspacePiResources,
	WorkspacePiResourcesUpdate,
} from "./types";

export async function getWorkspaceMeta(
	workspacePath: string,
): Promise<WorkspaceMeta> {
	const url = new URL(controlPlaneApiUrl("/api/workspace/meta"), window.location.origin);
	url.searchParams.set("workspace_path", workspacePath);
	const res = await authFetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function updateWorkspaceMeta(
	workspacePath: string,
	payload: WorkspaceMeta,
): Promise<WorkspaceMeta> {
	const url = new URL(controlPlaneApiUrl("/api/workspace/meta"), window.location.origin);
	url.searchParams.set("workspace_path", workspacePath);
	const res = await authFetch(url.toString(), {
		method: "PATCH",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(payload),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function getWorkspaceSandbox(
	workspacePath: string,
): Promise<WorkspaceSandboxConfig> {
	const url = new URL(controlPlaneApiUrl("/api/workspace/sandbox"), window.location.origin);
	url.searchParams.set("workspace_path", workspacePath);
	const res = await authFetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function updateWorkspaceSandbox(
	workspacePath: string,
	payload: { profile: string },
): Promise<WorkspaceSandboxConfig> {
	const url = new URL(controlPlaneApiUrl("/api/workspace/sandbox"), window.location.origin);
	url.searchParams.set("workspace_path", workspacePath);
	const res = await authFetch(url.toString(), {
		method: "PATCH",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(payload),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function getWorkspacePiResources(
	workspacePath: string,
): Promise<WorkspacePiResources> {
	const url = new URL(
		controlPlaneApiUrl("/api/workspace/pi-resources"),
		window.location.origin,
	);
	url.searchParams.set("workspace_path", workspacePath);
	const res = await authFetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function applyWorkspacePiResources(
	payload: WorkspacePiResourcesUpdate,
): Promise<WorkspacePiResources> {
	const res = await authFetch(controlPlaneApiUrl("/api/workspace/pi-resources"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(payload),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}
