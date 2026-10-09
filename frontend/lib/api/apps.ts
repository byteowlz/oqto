import type { AppCandidateList } from "@/src/generated/AppCandidateList";
import type { AppInstanceList } from "@/src/generated/AppInstanceList";
import type { AppPermissionDecision } from "@/src/generated/AppPermissionDecision";
import type { AppPermissionStatus } from "@/src/generated/AppPermissionStatus";
import type { AppPresentationDocument } from "@/src/generated/AppPresentationDocument";
import type { AppPublishResult } from "@/src/generated/AppPublishResult";
import { authFetch, controlPlaneApiUrl } from "./client";

export const APP_PERMISSION_CHANGED_EVENT = "oqto:app-permission-changed";

export interface AppPermissionChangedDetail {
	instanceId: string;
	state: AppPermissionStatus["state"];
}

function announcePermissionChange(detail: AppPermissionChangedDetail): void {
	window.dispatchEvent(
		new CustomEvent<AppPermissionChangedDetail>(APP_PERMISSION_CHANGED_EVENT, {
			detail,
		}),
	);
	try {
		const channel = new BroadcastChannel(APP_PERMISSION_CHANGED_EVENT);
		channel.postMessage(detail);
		channel.close();
	} catch {
		// The current document still receives the synchronous lifecycle event.
	}
}

async function appJson<T>(response: Response): Promise<T> {
	if (response.ok) return response.json() as Promise<T>;
	let message = `Oqto App request failed (${response.status})`;
	try {
		const payload = (await response.json()) as {
			error?: string;
			message?: string;
		};
		message = payload.error ?? payload.message ?? message;
	} catch {
		// Preserve the bounded status-only fallback when the response is not JSON.
	}
	throw new Error(message);
}

function workDirectoryQuery(workspacePath: string): string {
	return new URLSearchParams({ workspace_path: workspacePath }).toString();
}

export async function listAppCandidates(
	workspacePath: string,
): Promise<AppCandidateList> {
	const response = await authFetch(
		controlPlaneApiUrl(
			`/api/apps/candidates?${workDirectoryQuery(workspacePath)}`,
		),
	);
	return appJson<AppCandidateList>(response);
}

export async function publishApp(
	workspacePath: string,
	appId: string,
): Promise<AppPublishResult> {
	const response = await authFetch(controlPlaneApiUrl("/api/apps/publish"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ workspace_path: workspacePath, app_id: appId }),
	});
	return appJson<AppPublishResult>(response);
}

export async function listAppInstances(
	workspacePath: string,
): Promise<AppInstanceList> {
	const response = await authFetch(
		controlPlaneApiUrl(
			`/api/apps/instances?${workDirectoryQuery(workspacePath)}`,
		),
	);
	return appJson<AppInstanceList>(response);
}

export async function getAppPermissions(
	workspacePath: string,
	instanceId: string,
): Promise<AppPermissionStatus> {
	const encodedId = encodeURIComponent(instanceId);
	const response = await authFetch(
		controlPlaneApiUrl(
			`/api/apps/instances/${encodedId}/permissions?${workDirectoryQuery(workspacePath)}`,
		),
	);
	return appJson<AppPermissionStatus>(response);
}

export async function decideAppPermissions(
	workspacePath: string,
	instanceId: string,
	decision: AppPermissionDecision,
	reviewedContentDigest: string,
): Promise<AppPermissionStatus> {
	const encodedId = encodeURIComponent(instanceId);
	const response = await authFetch(
		controlPlaneApiUrl(`/api/apps/instances/${encodedId}/permissions/decision`),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				workspace_path: workspacePath,
				decision,
				reviewed_content_digest: reviewedContentDigest,
			}),
		},
	);
	return appJson<AppPermissionStatus>(response);
}

export async function revokeAppPermissions(
	workspacePath: string,
	instanceId: string,
): Promise<AppPermissionStatus> {
	const encodedId = encodeURIComponent(instanceId);
	const response = await authFetch(
		controlPlaneApiUrl(`/api/apps/instances/${encodedId}/permissions/revoke`),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ workspace_path: workspacePath }),
		},
	);
	const status = await appJson<AppPermissionStatus>(response);
	announcePermissionChange({ instanceId, state: status.state });
	return status;
}

export interface AppFileResource {
	role: string;
	reference: string;
	label: string;
	access: "read" | "readwrite";
	kind: "document" | "collection";
	watch: boolean;
}

export interface AppFileEntry {
	reference: string;
	label: string;
	media_type: string;
	version: string;
	size: number;
	is_directory: boolean;
	modified_at?: string;
}

export interface AppFileContents extends AppFileEntry {
	bytes_base64: string;
}

async function appFilePost<T>(
	workspacePath: string,
	instanceId: string,
	action: "resources" | "list" | "read",
	reference?: string,
): Promise<T> {
	const encodedId = encodeURIComponent(instanceId);
	const response = await authFetch(
		controlPlaneApiUrl(`/api/apps/instances/${encodedId}/files/${action}`),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ workspace_path: workspacePath, reference }),
		},
	);
	return appJson<T>(response);
}

export async function getAppFileResources(
	workspacePath: string,
	instanceId: string,
) {
	return (
		await appFilePost<{ resources: AppFileResource[] }>(
			workspacePath,
			instanceId,
			"resources",
		)
	).resources;
}

export async function listAppFiles(
	workspacePath: string,
	instanceId: string,
	reference: string,
) {
	return (
		await appFilePost<{ entries: AppFileEntry[] }>(
			workspacePath,
			instanceId,
			"list",
			reference,
		)
	).entries;
}

export function readAppFile(
	workspacePath: string,
	instanceId: string,
	reference: string,
) {
	return appFilePost<AppFileContents>(
		workspacePath,
		instanceId,
		"read",
		reference,
	);
}

export async function writeAppFile(
	workspacePath: string,
	instanceId: string,
	reference: string,
	expectedVersion: string,
	bytes: Uint8Array,
): Promise<{ written: boolean; version: string }> {
	let binary = "";
	for (let offset = 0; offset < bytes.length; offset += 0x8000) {
		binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
	}
	const encodedId = encodeURIComponent(instanceId);
	const response = await authFetch(
		controlPlaneApiUrl(`/api/apps/instances/${encodedId}/files/write`),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				workspace_path: workspacePath,
				reference,
				expected_version: expectedVersion,
				bytes_base64: btoa(binary),
			}),
		},
	);
	return appJson(response);
}

export interface AppOperationResult {
	ok: boolean;
	code: string;
	message: string;
	output: unknown;
}

export async function invokeAppOperation(
	workspacePath: string,
	instanceId: string,
	operationId: string,
	input: unknown,
	signal?: AbortSignal,
): Promise<AppOperationResult> {
	const encodedId = encodeURIComponent(instanceId);
	const response = await authFetch(
		controlPlaneApiUrl(`/api/apps/instances/${encodedId}/operations/invoke`),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				workspace_path: workspacePath,
				operation_id: operationId,
				input,
			}),
			signal,
		},
	);
	return appJson<AppOperationResult>(response);
}

export async function getAppKv(
	workspacePath: string,
	instanceId: string,
	key: string,
): Promise<unknown | undefined> {
	const encodedId = encodeURIComponent(instanceId);
	const query = new URLSearchParams({ workspace_path: workspacePath, key });
	const response = await authFetch(
		controlPlaneApiUrl(`/api/apps/instances/${encodedId}/kv?${query}`),
	);
	const payload = await appJson<{ value?: unknown }>(response);
	return payload.value;
}

export async function setAppKv(
	workspacePath: string,
	instanceId: string,
	key: string,
	value: unknown,
): Promise<void> {
	const encodedId = encodeURIComponent(instanceId);
	const response = await authFetch(
		controlPlaneApiUrl(`/api/apps/instances/${encodedId}/kv`),
		{
			method: "PUT",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ workspace_path: workspacePath, key, value }),
		},
	);
	await appJson<{ value?: unknown }>(response);
}

export async function deleteAppKv(
	workspacePath: string,
	instanceId: string,
	key: string,
): Promise<void> {
	const encodedId = encodeURIComponent(instanceId);
	const response = await authFetch(
		controlPlaneApiUrl(`/api/apps/instances/${encodedId}/kv`),
		{
			method: "DELETE",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ workspace_path: workspacePath, key }),
		},
	);
	await appJson<{ value?: unknown }>(response);
}

export async function fetchAppPresentation(
	workspacePath: string,
	instanceId: string,
): Promise<AppPresentationDocument> {
	const encodedId = encodeURIComponent(instanceId);
	const response = await authFetch(
		controlPlaneApiUrl(`/api/apps/instances/${encodedId}/presentation`),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ workspace_path: workspacePath }),
		},
	);
	return appJson<AppPresentationDocument>(response);
}
