import type { AppCandidateList } from "@/src/generated/AppCandidateList";
import type { AppInstanceList } from "@/src/generated/AppInstanceList";
import type { AppPermissionDecision } from "@/src/generated/AppPermissionDecision";
import type { AppPermissionStatus } from "@/src/generated/AppPermissionStatus";
import type { AppPresentationDocument } from "@/src/generated/AppPresentationDocument";
import type { AppPublishResult } from "@/src/generated/AppPublishResult";
import { authFetch, controlPlaneApiUrl } from "./client";

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
	return appJson<AppPermissionStatus>(response);
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
