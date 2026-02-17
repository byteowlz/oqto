/**
 * Admin API hooks for managing sessions, users, metrics, and invite codes.
 */

import { controlPlaneApiUrl, getAuthHeaders } from "@/lib/control-plane-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";

// ============================================================================
// Types
// ============================================================================

export type SessionStatus =
	| "pending"
	| "starting"
	| "running"
	| "stopping"
	| "stopped"
	| "failed";

export type RuntimeMode = "container" | "local";

export type AdminSession = {
	id: string;
	container_id: string | null;
	container_name: string;
	user_id: string;
	workspace_path: string;
	agent: string | null;
	image: string;
	image_digest: string | null;
	agent_port: number;
	fileserver_port: number;
	ttyd_port: number;
	eavs_port: number | null;
	agent_base_port: number | null;
	max_agents: number | null;
	eavs_key_id: string | null;
	mmry_port: number | null;
	status: SessionStatus;
	runtime_mode: RuntimeMode;
	created_at: string;
	started_at: string | null;
	stopped_at: string | null;
	last_activity_at: string | null;
	error_message: string | null;
};

export type UserRole = "user" | "admin" | "service";

export type AdminUser = {
	id: string;
	username: string;
	email: string;
	display_name: string;
	avatar_url: string | null;
	role: UserRole;
	is_active: boolean;
	created_at: string;
	last_login_at: string | null;
};

export type UserStats = {
	total: number;
	active: number;
	admins: number;
	users: number;
	services: number;
};

export type CreateUserRequest = {
	username: string;
	email: string;
	password?: string;
	display_name?: string;
	role?: UserRole;
};

export type UpdateUserRequest = {
	username?: string;
	email?: string;
	password?: string;
	display_name?: string;
	avatar_url?: string;
	role?: UserRole;
	is_active?: boolean;
};

export type InviteCode = {
	id: string;
	code: string;
	created_by: string;
	uses_remaining: number;
	max_uses: number;
	expires_at: string | null;
	created_at: string;
	is_valid: boolean;
	note: string | null;
};

export type InviteCodeStats = {
	total: number;
	valid: number;
};

export type CreateInviteCodeRequest = {
	code?: string;
	max_uses?: number;
	expires_in_secs?: number;
	note?: string;
};

export type BatchCreateInviteCodesRequest = {
	count: number;
	uses_per_code?: number;
	expires_in_secs?: number;
	prefix?: string;
	note?: string;
};

export type HostMetrics = {
	cpu_percent: number;
	mem_total_bytes: number;
	mem_used_bytes: number;
	mem_available_bytes: number;
};

export type ContainerStats = {
	container_id: string;
	name: string;
	cpu_percent: string;
	mem_usage: string;
	mem_percent: string;
	net_io: string;
	block_io: string;
	pids: string;
};

export type SessionContainerStats = {
	session_id: string;
	container_id: string;
	container_name: string;
	stats: ContainerStats;
};

export type AdminMetricsSnapshot = {
	timestamp: string;
	host: HostMetrics | null;
	containers: SessionContainerStats[];
	error: string | null;
};

// ============================================================================
// Query Keys
// ============================================================================

export const adminKeys = {
	all: ["admin"] as const,
	sessions: () => [...adminKeys.all, "sessions"] as const,
	users: () => [...adminKeys.all, "users"] as const,
	userStats: () => [...adminKeys.all, "userStats"] as const,
	inviteCodes: () => [...adminKeys.all, "inviteCodes"] as const,
	inviteCodeStats: () => [...adminKeys.all, "inviteCodeStats"] as const,
	eavsProviders: () => [...adminKeys.all, "eavsProviders"] as const,
};

// ============================================================================
// Fetch Helpers
// ============================================================================

async function authFetch(
	input: RequestInfo | URL,
	init?: RequestInit,
): Promise<Response> {
	const headers = {
		...getAuthHeaders(),
		...(init?.headers instanceof Headers
			? Object.fromEntries(init.headers.entries())
			: (init?.headers as Record<string, string> | undefined)),
	};
	return fetch(input, {
		...init,
		headers,
		credentials: "include",
	});
}

async function readApiError(res: Response): Promise<string> {
	const contentType = res.headers.get("content-type") ?? "";
	if (contentType.includes("application/json")) {
		const parsed = await res.json().catch(() => null);
		if (parsed?.error) return parsed.error;
	}
	return (await res.text().catch(() => res.statusText)) || res.statusText;
}

// ============================================================================
// Session API
// ============================================================================

async function fetchAdminSessions(): Promise<AdminSession[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/admin/sessions"));
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function forceStopSession(sessionId: string): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/admin/sessions/${sessionId}`),
		{ method: "DELETE" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

export function useAdminSessions() {
	return useQuery({
		queryKey: adminKeys.sessions(),
		queryFn: fetchAdminSessions,
		refetchInterval: 10000, // Refresh every 10s
	});
}

export function useForceStopSession() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: forceStopSession,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.sessions() });
		},
	});
}

// ============================================================================
// User API
// ============================================================================

async function fetchAdminUsers(): Promise<AdminUser[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/admin/users"));
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function fetchUserStats(): Promise<UserStats> {
	const res = await authFetch(controlPlaneApiUrl("/api/admin/users/stats"));
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function createUser(request: CreateUserRequest): Promise<AdminUser> {
	const res = await authFetch(controlPlaneApiUrl("/api/admin/users"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function updateUser(
	userId: string,
	request: UpdateUserRequest,
): Promise<AdminUser> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/admin/users/${userId}`),
		{
			method: "PUT",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(request),
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function deleteUser(userId: string): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/admin/users/${userId}`),
		{ method: "DELETE" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

async function activateUser(userId: string): Promise<AdminUser> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/admin/users/${userId}/activate`),
		{ method: "POST" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function deactivateUser(userId: string): Promise<AdminUser> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/admin/users/${userId}/deactivate`),
		{ method: "POST" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

export function useAdminUsers() {
	return useQuery({
		queryKey: adminKeys.users(),
		queryFn: fetchAdminUsers,
	});
}

export function useUserStats() {
	return useQuery({
		queryKey: adminKeys.userStats(),
		queryFn: fetchUserStats,
	});
}

export function useCreateUser() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: createUser,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.users() });
			queryClient.invalidateQueries({ queryKey: adminKeys.userStats() });
		},
	});
}

export function useUpdateUser() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: ({
			userId,
			request,
		}: { userId: string; request: UpdateUserRequest }) =>
			updateUser(userId, request),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.users() });
			queryClient.invalidateQueries({ queryKey: adminKeys.userStats() });
		},
	});
}

export function useDeleteUser() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: deleteUser,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.users() });
			queryClient.invalidateQueries({ queryKey: adminKeys.userStats() });
		},
	});
}

export function useActivateUser() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: activateUser,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.users() });
			queryClient.invalidateQueries({ queryKey: adminKeys.userStats() });
		},
	});
}

export function useDeactivateUser() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: deactivateUser,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.users() });
			queryClient.invalidateQueries({ queryKey: adminKeys.userStats() });
		},
	});
}

// ============================================================================
// Invite Code API
// ============================================================================

async function fetchInviteCodes(): Promise<InviteCode[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/admin/invite-codes"));
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function fetchInviteCodeStats(): Promise<InviteCodeStats> {
	const res = await authFetch(
		controlPlaneApiUrl("/api/admin/invite-codes/stats"),
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function createInviteCode(
	request: CreateInviteCodeRequest,
): Promise<InviteCode> {
	const res = await authFetch(controlPlaneApiUrl("/api/admin/invite-codes"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function createInviteCodesBatch(
	request: BatchCreateInviteCodesRequest,
): Promise<InviteCode[]> {
	const res = await authFetch(
		controlPlaneApiUrl("/api/admin/invite-codes/batch"),
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(request),
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

async function revokeInviteCode(codeId: string): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/admin/invite-codes/${codeId}/revoke`),
		{ method: "POST" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

async function deleteInviteCode(codeId: string): Promise<void> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/admin/invite-codes/${codeId}`),
		{ method: "DELETE" },
	);
	if (!res.ok) throw new Error(await readApiError(res));
}

export function useInviteCodes() {
	return useQuery({
		queryKey: adminKeys.inviteCodes(),
		queryFn: fetchInviteCodes,
	});
}

export function useInviteCodeStats() {
	return useQuery({
		queryKey: adminKeys.inviteCodeStats(),
		queryFn: fetchInviteCodeStats,
	});
}

export function useCreateInviteCode() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: createInviteCode,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.inviteCodes() });
			queryClient.invalidateQueries({ queryKey: adminKeys.inviteCodeStats() });
		},
	});
}

export function useCreateInviteCodesBatch() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: createInviteCodesBatch,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.inviteCodes() });
			queryClient.invalidateQueries({ queryKey: adminKeys.inviteCodeStats() });
		},
	});
}

export function useRevokeInviteCode() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: revokeInviteCode,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.inviteCodes() });
			queryClient.invalidateQueries({ queryKey: adminKeys.inviteCodeStats() });
		},
	});
}

export function useDeleteInviteCode() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: deleteInviteCode,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.inviteCodes() });
			queryClient.invalidateQueries({ queryKey: adminKeys.inviteCodeStats() });
		},
	});
}

// ============================================================================
// Metrics SSE Hook
// ============================================================================

export function useAdminMetrics() {
	const [metrics, setMetrics] = useState<AdminMetricsSnapshot | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [isConnected, setIsConnected] = useState(false);
	const eventSourceRef = useRef<EventSource | null>(null);

	const connect = useCallback(() => {
		if (eventSourceRef.current) {
			eventSourceRef.current.close();
		}

		const url = controlPlaneApiUrl("/api/admin/metrics");
		const eventSource = new EventSource(url, { withCredentials: true });
		eventSourceRef.current = eventSource;

		eventSource.onopen = () => {
			setIsConnected(true);
			setError(null);
		};

		eventSource.onmessage = (event) => {
			try {
				const data = JSON.parse(event.data) as AdminMetricsSnapshot;
				setMetrics(data);
				if (data.error) {
					setError(data.error);
				}
			} catch (err) {
				console.error("Failed to parse metrics:", err);
			}
		};

		eventSource.onerror = () => {
			setIsConnected(false);
			setError("Connection lost. Reconnecting...");
			eventSource.close();
			// Reconnect after 5 seconds
			setTimeout(connect, 5000);
		};
	}, []);

	const disconnect = useCallback(() => {
		if (eventSourceRef.current) {
			eventSourceRef.current.close();
			eventSourceRef.current = null;
		}
		setIsConnected(false);
	}, []);

	useEffect(() => {
		connect();
		return disconnect;
	}, [connect, disconnect]);

	return { metrics, error, isConnected, reconnect: connect };
}

// ============================================================================
// EAVS / Model Provider Types & Hooks
// ============================================================================

export type EavsModelSummary = {
	id: string;
	name: string;
	reasoning: boolean;
};

export type EavsProviderSummary = {
	name: string;
	type: string;
	pi_api: string | null;
	has_api_key: boolean;
	model_count: number;
	models: EavsModelSummary[];
};

export type EavsProvidersResponse = {
	providers: EavsProviderSummary[];
	eavs_url: string;
};

export type SyncUserConfigResult = {
	user_id: string;
	linux_username: string | null;
	runner_configured: boolean;
	mmry_configured: boolean;
	eavs_configured: boolean;
	error: string | null;
};

export type SyncUserConfigsResponse = {
	results: SyncUserConfigResult[];
};

async function fetchEavsProviders(): Promise<EavsProvidersResponse> {
	const res = await fetch(controlPlaneApiUrl("/api/admin/eavs/providers"), {
		headers: getAuthHeaders(),
	});
	if (!res.ok) {
		const text = await res.text();
		throw new Error(text || `HTTP ${res.status}`);
	}
	return res.json();
}

async function syncUserConfigs(
	userId?: string,
): Promise<SyncUserConfigsResponse> {
	const res = await fetch(
		controlPlaneApiUrl("/api/admin/users/sync-configs"),
		{
			method: "POST",
			headers: {
				...getAuthHeaders(),
				"Content-Type": "application/json",
			},
			body: JSON.stringify(userId ? { user_id: userId } : {}),
		},
	);
	if (!res.ok) {
		const text = await res.text();
		throw new Error(text || `HTTP ${res.status}`);
	}
	return res.json();
}

export function useEavsProviders() {
	return useQuery({
		queryKey: adminKeys.eavsProviders(),
		queryFn: fetchEavsProviders,
		staleTime: 30_000,
	});
}

export function useSyncUserConfigs() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (userId?: string) => syncUserConfigs(userId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: adminKeys.users() });
		},
	});
}
