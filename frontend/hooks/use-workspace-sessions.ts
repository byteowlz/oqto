import {
	type CreateWorkspaceSessionRequest,
	type WorkspaceSession,
	createWorkspaceSession,
	listWorkspaceSessions,
	stopWorkspaceSession,
} from "@/lib/control-plane-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

// Query keys
export const workspaceSessionKeys = {
	all: ["workspace-sessions"] as const,
	lists: () => [...workspaceSessionKeys.all, "list"] as const,
	list: (filters: Record<string, unknown>) =>
		[...workspaceSessionKeys.lists(), filters] as const,
	details: () => [...workspaceSessionKeys.all, "detail"] as const,
	detail: (id: string) => [...workspaceSessionKeys.details(), id] as const,
};

// Hook to list all workspace sessions
export function useWorkspaceSessions(options?: { enabled?: boolean }) {
	return useQuery({
		queryKey: workspaceSessionKeys.lists(),
		queryFn: listWorkspaceSessions,
		enabled: options?.enabled ?? true,
	});
}

// Hook to get a specific session from the cached list
export function useWorkspaceSession(sessionId: string | undefined) {
	return useQuery({
		queryKey: workspaceSessionKeys.detail(sessionId ?? ""),
		queryFn: async () => {
			const sessions = await listWorkspaceSessions();
			return sessions.find((s) => s.id === sessionId) ?? null;
		},
		enabled: !!sessionId,
	});
}

// Hook to create a new workspace session
export function useCreateWorkspaceSession() {
	const queryClient = useQueryClient();

	return useMutation({
		mutationFn: (request: CreateWorkspaceSessionRequest) =>
			createWorkspaceSession(request),
		onSuccess: (newSession) => {
			// Update the sessions list cache
			queryClient.setQueryData<WorkspaceSession[]>(
				workspaceSessionKeys.lists(),
				(old) => {
					if (!old) return [newSession];
					return [newSession, ...old];
				},
			);
		},
		onError: (error) => {
			console.error("Failed to create workspace session:", error);
		},
	});
}

// Hook to stop a workspace session
export function useStopWorkspaceSession() {
	const queryClient = useQueryClient();

	return useMutation({
		mutationFn: (sessionId: string) => stopWorkspaceSession(sessionId),
		onMutate: async (sessionId) => {
			// Cancel any outgoing refetches
			await queryClient.cancelQueries({
				queryKey: workspaceSessionKeys.lists(),
			});

			// Snapshot the previous value
			const previousSessions = queryClient.getQueryData<WorkspaceSession[]>(
				workspaceSessionKeys.lists(),
			);

			// Optimistically update to the stopped status
			queryClient.setQueryData<WorkspaceSession[]>(
				workspaceSessionKeys.lists(),
				(old) => {
					if (!old) return old;
					return old.map((session) =>
						session.id === sessionId
							? { ...session, status: "stopping" as const }
							: session,
					);
				},
			);

			return { previousSessions };
		},
		onError: (_err, _sessionId, context) => {
			// Roll back to the previous value on error
			if (context?.previousSessions) {
				queryClient.setQueryData(
					workspaceSessionKeys.lists(),
					context.previousSessions,
				);
			}
		},
		onSettled: () => {
			// Always refetch after error or success
			queryClient.invalidateQueries({ queryKey: workspaceSessionKeys.lists() });
		},
	});
}

// Hook to refresh sessions list
export function useRefreshWorkspaceSessions() {
	const queryClient = useQueryClient();

	return () => {
		queryClient.invalidateQueries({ queryKey: workspaceSessionKeys.lists() });
	};
}
