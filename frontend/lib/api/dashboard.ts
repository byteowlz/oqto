/**
 * Dashboard APIs
 * Scheduler, feeds, codexbar usage
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";

// ============================================================================
// Dashboard Types
// ============================================================================

export type SchedulerEntry = {
	name: string;
	status: string;
	schedule: string;
	command: string;
	next_run?: string | null;
};

export type SchedulerOverview = {
	stats: {
		total: number;
		enabled: number;
		disabled: number;
	};
	schedules: SchedulerEntry[];
};

export type FeedFetchResponse = {
	url: string;
	content: string;
	content_type?: string | null;
};

export type CodexBarUsagePayload = {
	provider: string;
	account?: string | null;
	version?: string | null;
	source?: string | null;
	status?: {
		indicator?: string | null;
		description?: string | null;
		updatedAt?: string | null;
		url?: string | null;
	} | null;
	usage?: {
		primary?: {
			usedPercent?: number | null;
			windowMinutes?: number | null;
			resetsAt?: string | null;
		} | null;
		secondary?: {
			usedPercent?: number | null;
			windowMinutes?: number | null;
			resetsAt?: string | null;
		} | null;
		updatedAt?: string | null;
		accountEmail?: string | null;
		accountOrganization?: string | null;
		loginMethod?: string | null;
	} | null;
	credits?: {
		remaining?: number | null;
		updatedAt?: string | null;
	} | null;
	error?: {
		message?: string | null;
	} | null;
};

// ============================================================================
// Dashboard APIs
// ============================================================================

export async function getSchedulerOverview(): Promise<SchedulerOverview> {
	const res = await authFetch(controlPlaneApiUrl("/api/scheduler/overview"), {
		credentials: "include",
	});
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

export async function deleteSchedulerJob(
	name: string,
): Promise<{ deleted: string }> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/scheduler/jobs/${encodeURIComponent(name)}`),
		{
			method: "DELETE",
			credentials: "include",
		},
	);
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

export async function fetchFeed(url: string): Promise<FeedFetchResponse> {
	const endpoint = controlPlaneApiUrl(
		`/api/feeds/fetch?url=${encodeURIComponent(url)}`,
	);
	const res = await authFetch(endpoint, { credentials: "include" });
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

export async function getCodexBarUsage(): Promise<
	CodexBarUsagePayload[] | null
> {
	const res = await authFetch(controlPlaneApiUrl("/api/codexbar/usage"), {
		credentials: "include",
	});
	if (res.status === 404) return null;
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}
