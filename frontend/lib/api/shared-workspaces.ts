/**
 * Shared Workspaces API
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";

// ============================================================================
// Types
// ============================================================================

export type MemberRole = "owner" | "admin" | "member" | "viewer";

export type SharedWorkspaceInfo = {
	id: string;
	name: string;
	slug: string;
	path: string;
	owner_id: string;
	description: string | null;
	icon: string;
	color: string;
	created_at: string;
	updated_at: string;
	my_role: MemberRole;
	member_count: number;
};

export type SharedWorkspaceMemberInfo = {
	user_id: string;
	display_name: string;
	role: MemberRole;
	joined_at: string;
};

export type CreateSharedWorkspaceRequest = {
	name: string;
	description?: string;
	icon?: string;
	color?: string;
	member_ids?: string[];
};

export type UpdateSharedWorkspaceRequest = {
	name?: string;
	description?: string;
	icon?: string;
	color?: string;
};

export type AddMemberRequest = {
	user_id: string;
	role?: MemberRole;
};

export type UpdateMemberRoleRequest = {
	role: MemberRole;
};

export type ConvertToSharedRequest = {
	source_path: string;
	name: string;
	description?: string;
	icon?: string;
	color?: string;
	member_ids?: string[];
};

export type CreateSharedWorkspaceWorkdirRequest = {
	source_path: string;
	name?: string;
};

export type TransferOwnershipRequest = {
	new_owner_id: string;
};

export type SharedWorkspaceUpdatedEvent = {
	workspace_id: string;
	change_type:
		| "member_added"
		| "member_removed"
		| "member_role_changed"
		| "workspace_updated"
		| "workspace_deleted";
	detail: string | null;
};

// ============================================================================
// Available icons and colors (must match backend constants)
// ============================================================================

export const WORKSPACE_ICONS = [
	"users",
	"rocket",
	"globe",
	"code",
	"building",
	"shield",
	"zap",
	"layers",
	"hexagon",
	"terminal",
	"flask-conical",
	"palette",
	"brain",
	"database",
	"network",
	"git-branch",
] as const;

export type WorkspaceIconName = (typeof WORKSPACE_ICONS)[number];

export const WORKSPACE_COLORS = [
	"#3ba77c", // primary green (matches theme primary)
	"#5b8a72", // sage
	"#7c9a92", // eucalyptus
	"#6b8f9c", // slate teal
	"#5c7d8a", // steel blue
	"#7b8fa6", // dusty blue
	"#8b7fa3", // muted violet
	"#9c7b8f", // mauve
	"#a67c7c", // dusty rose
	"#b0926b", // warm sand
	"#8a9670", // olive
	"#6b9080", // seafoam
] as const;

export type WorkspaceColor = (typeof WORKSPACE_COLORS)[number];

// ============================================================================
// API Functions
// ============================================================================

function apiUrl(path: string): string {
	return new URL(
		controlPlaneApiUrl(`/api/shared-workspaces${path}`),
		window.location.origin,
	).toString();
}

/** List all shared workspaces the current user belongs to. */
export async function listSharedWorkspaces(): Promise<SharedWorkspaceInfo[]> {
	const res = await authFetch(apiUrl(""), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Create a new shared workspace. */
export async function createSharedWorkspace(
	request: CreateSharedWorkspaceRequest,
): Promise<SharedWorkspaceInfo> {
	const res = await authFetch(apiUrl(""), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get a shared workspace by ID. */
export async function getSharedWorkspace(
	workspaceId: string,
): Promise<SharedWorkspaceInfo> {
	const res = await authFetch(apiUrl(`/${workspaceId}`), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Update a shared workspace. */
export async function updateSharedWorkspace(
	workspaceId: string,
	request: UpdateSharedWorkspaceRequest,
): Promise<SharedWorkspaceInfo> {
	const res = await authFetch(apiUrl(`/${workspaceId}`), {
		method: "PATCH",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Delete a shared workspace. */
export async function deleteSharedWorkspace(
	workspaceId: string,
): Promise<void> {
	const res = await authFetch(apiUrl(`/${workspaceId}`), {
		method: "DELETE",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** List members of a shared workspace. */
export async function listMembers(
	workspaceId: string,
): Promise<SharedWorkspaceMemberInfo[]> {
	const res = await authFetch(apiUrl(`/${workspaceId}/members`), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Add a member to a shared workspace. */
export async function addMember(
	workspaceId: string,
	request: AddMemberRequest,
): Promise<SharedWorkspaceMemberInfo> {
	const res = await authFetch(apiUrl(`/${workspaceId}/members`), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Update a member's role. */
export async function updateMemberRole(
	workspaceId: string,
	userId: string,
	request: UpdateMemberRoleRequest,
): Promise<void> {
	const res = await authFetch(apiUrl(`/${workspaceId}/members/${userId}`), {
		method: "PATCH",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Remove a member from a shared workspace. */
export async function removeMember(
	workspaceId: string,
	userId: string,
): Promise<void> {
	const res = await authFetch(apiUrl(`/${workspaceId}/members/${userId}`), {
		method: "DELETE",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}

/** Convert a personal project into a shared workspace. */
export async function convertToSharedWorkspace(
	request: ConvertToSharedRequest,
): Promise<SharedWorkspaceInfo> {
	const res = await authFetch(apiUrl("/convert"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Add a workdir to an existing shared workspace by copying a source path. */
export async function createSharedWorkspaceWorkdir(
	workspaceId: string,
	request: CreateSharedWorkspaceWorkdirRequest,
): Promise<{ workspace_id: string; workdir_path: string }> {
	const res = await authFetch(apiUrl(`/${workspaceId}/workdirs`), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Workdir (project directory) in a shared workspace. */
export type SharedWorkspaceWorkdir = {
	name: string;
	path: string;
};

/** List workdirs in a shared workspace. */
export async function listWorkdirs(
	workspaceId: string,
): Promise<SharedWorkspaceWorkdir[]> {
	const res = await authFetch(apiUrl(`/${workspaceId}/workdirs`), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Transfer ownership of a shared workspace. */
export async function transferOwnership(
	workspaceId: string,
	request: TransferOwnershipRequest,
): Promise<void> {
	const res = await authFetch(apiUrl(`/${workspaceId}/transfer-ownership`), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
}
