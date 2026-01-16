import {
	type UserInfo,
	logout as apiLogout,
	controlPlaneApiUrl,
	getAuthHeaders,
	setAuthToken,
} from "@/lib/control-plane-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

// Query keys
export const authKeys = {
	all: ["auth"] as const,
	me: () => [...authKeys.all, "me"] as const,
};

async function fetchCurrentUser(): Promise<UserInfo | null> {
	const headers = getAuthHeaders();
	const url = controlPlaneApiUrl("/api/me");
	// Only log in development - headers contain sensitive auth tokens
	if (import.meta.env.DEV) {
		console.log("[auth] fetchCurrentUser", url);
	}

	// Retry logic for transient network errors
	const maxRetries = 2;
	let lastError: Error | undefined;

	for (let attempt = 0; attempt <= maxRetries; attempt++) {
		try {
			const response = await fetch(url, {
				headers: { ...headers },
				credentials: "include",
			});
			if (import.meta.env.DEV) {
				console.log("[auth] /me response:", response.status);
			}
			if (response.status === 401) {
				// Clear any stale token
				if (import.meta.env.DEV) {
					console.log("[auth] 401 - clearing token");
				}
				setAuthToken(null);
				return null;
			}
			if (!response.ok) {
				// Non-auth errors - return null to show login screen
				// This allows user to configure backend URL
				console.warn("[auth] Non-OK response from /me:", response.status);
				return null;
			}
			const data = await response.json();
			// Only log user ID in dev, not full user object (contains PII)
			if (import.meta.env.DEV) {
				console.log("[auth] /me user id:", data?.id);
			}
			return data;
		} catch (error) {
			lastError = error instanceof Error ? error : new Error(String(error));
			// Only retry on network errors
			const isNetworkError =
				lastError.message.includes("Failed to fetch") ||
				lastError.message.includes("NetworkError") ||
				lastError.message.includes("network") ||
				lastError.name === "TypeError";

			if (!isNetworkError || attempt === maxRetries) {
				// If the API is not reachable, return null to show login screen
				// This allows user to configure backend URL on mobile
				console.warn("[auth] Failed to reach backend:", error);
				return null;
			}

			if (import.meta.env.DEV) {
				console.log(
					`[auth] Network error, retrying (attempt ${attempt + 1}/${maxRetries})`,
				);
			}
			// Wait before retrying
			await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
		}
	}

	return null;
}

/**
 * Hook to get the current authenticated user.
 * Returns cached data when available to avoid spinner on tab focus.
 */
export function useCurrentUser() {
	return useQuery({
		queryKey: authKeys.me(),
		queryFn: fetchCurrentUser,
		staleTime: 5 * 60 * 1000, // 5 minutes - don't refetch if data is fresh
		gcTime: 30 * 60 * 1000, // 30 minutes - keep in cache
		refetchOnWindowFocus: false, // Don't refetch on tab focus
		refetchOnReconnect: false, // Don't refetch on network reconnect
		retry: false, // Don't retry auth checks
	});
}

/**
 * Hook to log out the current user.
 * Invalidates the auth cache and redirects to login.
 */
export function useLogout() {
	const queryClient = useQueryClient();
	const navigate = useNavigate();

	return useMutation({
		mutationFn: apiLogout,
		onSuccess: () => {
			// Clear the auth cache
			queryClient.setQueryData(authKeys.me(), null);
			queryClient.invalidateQueries({ queryKey: authKeys.all });
			// Redirect to login
			navigate("/login");
		},
		onError: (error) => {
			console.error("Logout failed:", error);
			// Even on error, clear local state and redirect
			queryClient.setQueryData(authKeys.me(), null);
			navigate("/login");
		},
	});
}

/**
 * Hook to invalidate the auth cache.
 * Call this after login to refresh user data.
 */
export function useInvalidateAuth() {
	const queryClient = useQueryClient();

	return () => {
		queryClient.invalidateQueries({ queryKey: authKeys.all });
	};
}
