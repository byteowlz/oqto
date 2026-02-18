/**
 * Projects API
 * Workspace directories, project templates, project logos
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";
import type {
	CreateProjectFromTemplateRequest,
	ListProjectTemplatesResponse,
	ProjectEntry,
	WorkspaceDirEntry,
} from "./types";

/** List available projects (directories in workspace_dir) */
export async function listProjects(): Promise<ProjectEntry[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/projects"), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function listWorkspaceDirectories(
	path = ".",
): Promise<WorkspaceDirEntry[]> {
	const url = new URL(
		controlPlaneApiUrl("/api/projects"),
		window.location.origin,
	);
	url.searchParams.set("path", path);
	const res = await authFetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function listProjectTemplates(): Promise<ListProjectTemplatesResponse> {
	const url = new URL(
		controlPlaneApiUrl("/api/projects/templates"),
		window.location.origin,
	);
	const res = await authFetch(url.toString(), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export async function createProjectFromTemplate(
	payload: CreateProjectFromTemplateRequest,
): Promise<WorkspaceDirEntry> {
	const url = new URL(
		controlPlaneApiUrl("/api/projects/templates"),
		window.location.origin,
	);
	const res = await authFetch(url.toString(), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(payload),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/**
 * Get the URL for a project logo.
 * @param projectPath - The project path (relative to workspace root, e.g., "oqto" or "subfolder/project")
 * @param logoPath - The logo path relative to project root (e.g., "logo/project_logo_white.svg")
 */
export function getProjectLogoUrl(
	projectPath: string,
	logoPath: string,
): string {
	// Combine project path and logo path
	// The path should be relative to workspace root, not absolute
	const fullPath = `${projectPath}/${logoPath}`;
	return controlPlaneApiUrl(`/api/projects/logo/${fullPath}`);
}
