import { QueryClient } from "@tanstack/react-query";

export function makeQueryClient() {
	return new QueryClient({
		defaultOptions: {
			queries: {
				// With SSR, we usually want to set some default staleTime
				// above 0 to avoid refetching immediately on the client
				staleTime: 60 * 1000, // 1 minute
				refetchOnWindowFocus: false,
				retry: (failureCount, error) => {
					// Don't retry on 401/403 (auth errors) or 404 (not found)
					if (error instanceof Error) {
						const status = (error as { status?: number }).status;
						if (status === 401 || status === 403 || status === 404) {
							return false;
						}
					}
					return failureCount < 3;
				},
			},
			mutations: {
				retry: false,
			},
		},
	});
}

// Singleton for browser usage
let browserQueryClient: QueryClient | undefined = undefined;

export function getQueryClient() {
	if (typeof window === "undefined") {
		// Server: always make a new query client
		return makeQueryClient();
	}

	// Browser: make a new query client if we don't already have one
	// This is important so we don't re-make a new client if React
	// suspends during the initial render
	if (!browserQueryClient) browserQueryClient = makeQueryClient();
	return browserQueryClient;
}
