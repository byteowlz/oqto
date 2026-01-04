import { Spinner } from "@/components/ui/spinner";
import { useCurrentUser } from "@/hooks/use-auth";
import type React from "react";
import { useMemo } from "react";
import { Navigate, useLocation } from "react-router-dom";

export function RequireAuth({ children }: { children: React.ReactNode }) {
	const location = useLocation();
	const { data: user, isLoading, isFetched } = useCurrentUser();

	const redirectTarget = useMemo(() => {
		const target = location.pathname + location.search;
		return `/login?redirect=${encodeURIComponent(target)}`;
	}, [location.pathname, location.search]);

	// Only show spinner on initial load, not on subsequent navigations
	if (isLoading && !isFetched) {
		return (
			<div className="min-h-screen flex items-center justify-center bg-background text-foreground">
				<div className="flex items-center gap-2 text-sm text-muted-foreground">
					<Spinner className="size-4" />
					<span>Checking session...</span>
				</div>
			</div>
		);
	}

	// user is null means 401 response
	if (user === null) {
		return <Navigate to={redirectTarget} replace />;
	}

	return children;
}

// Re-export auth hooks for convenience
export { useCurrentUser, useLogout, useInvalidateAuth } from "@/hooks/use-auth";
