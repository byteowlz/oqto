import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  type OpenCodeSession,
  fetchSessions,
  fetchMessages,
  sendMessageAsync,
  createSession,
  abortSession,
} from "@/lib/opencode-client"

// Query keys
export const openCodeKeys = {
  all: ["opencode"] as const,
  sessions: (baseUrl: string) => [...openCodeKeys.all, "sessions", baseUrl] as const,
  session: (baseUrl: string, sessionId: string) => [...openCodeKeys.all, "session", baseUrl, sessionId] as const,
  messages: (baseUrl: string, sessionId: string) =>
    [...openCodeKeys.all, "messages", baseUrl, sessionId] as const,
}

// Hook to list all OpenCode sessions
export function useOpenCodeSessions(baseUrl: string | undefined, options?: { enabled?: boolean }) {
  return useQuery({
    queryKey: openCodeKeys.sessions(baseUrl ?? ""),
    queryFn: () => fetchSessions(baseUrl!),
    enabled: !!baseUrl && (options?.enabled ?? true),
    refetchInterval: 5000, // Poll every 5 seconds for new sessions
  })
}

// Hook to get messages for a specific session
export function useOpenCodeMessages(
  baseUrl: string | undefined,
  sessionId: string | undefined,
  options?: { enabled?: boolean; refetchInterval?: number },
) {
  return useQuery({
    queryKey: openCodeKeys.messages(baseUrl ?? "", sessionId ?? ""),
    queryFn: () => fetchMessages(baseUrl!, sessionId!),
    enabled: !!baseUrl && !!sessionId && (options?.enabled ?? true),
    refetchInterval: options?.refetchInterval ?? 2000, // Poll every 2 seconds for message updates
  })
}

// Hook to create a new OpenCode session
export function useCreateOpenCodeSession(baseUrl: string | undefined) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ title, parentID }: { title?: string; parentID?: string }) =>
      createSession(baseUrl!, title, parentID),
    onSuccess: (newSession) => {
      // Update the sessions list cache
      queryClient.setQueryData<OpenCodeSession[]>(openCodeKeys.sessions(baseUrl ?? ""), (old) => {
        if (!old) return [newSession]
        return [newSession, ...old]
      })
    },
    onError: (error) => {
      console.error("Failed to create OpenCode session:", error)
    },
  })
}

// Hook to send a message to a session
export function useSendMessage(baseUrl: string | undefined, sessionId: string | undefined) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({
      content,
      model,
    }: {
      content: string
      model?: { providerID: string; modelID: string }
    }) => sendMessageAsync(baseUrl!, sessionId!, content, model),
    onSuccess: () => {
      // Invalidate messages to trigger a refetch
      if (baseUrl && sessionId) {
        queryClient.invalidateQueries({
          queryKey: openCodeKeys.messages(baseUrl, sessionId),
        })
      }
    },
    onError: (error) => {
      console.error("Failed to send message:", error)
    },
  })
}

// Hook to abort a session
export function useAbortSession(baseUrl: string | undefined) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (sessionId: string) => abortSession(baseUrl!, sessionId),
    onSuccess: (_result, sessionId) => {
      // Invalidate messages to trigger a refetch
      if (baseUrl) {
        queryClient.invalidateQueries({
          queryKey: openCodeKeys.messages(baseUrl, sessionId),
        })
      }
    },
    onError: (error) => {
      console.error("Failed to abort session:", error)
    },
  })
}

// Hook to invalidate OpenCode data
export function useInvalidateOpenCode(baseUrl: string | undefined) {
  const queryClient = useQueryClient()

  return {
    invalidateSessions: () => {
      if (baseUrl) {
        queryClient.invalidateQueries({ queryKey: openCodeKeys.sessions(baseUrl) })
      }
    },
    invalidateMessages: (sessionId: string) => {
      if (baseUrl) {
        queryClient.invalidateQueries({ queryKey: openCodeKeys.messages(baseUrl, sessionId) })
      }
    },
    invalidateAll: () => {
      if (baseUrl) {
        queryClient.invalidateQueries({ queryKey: openCodeKeys.all })
      }
    },
  }
}
