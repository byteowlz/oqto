import { useCurrentUser } from "@/hooks/use-auth";
import type React from "react";
import { useMemo } from "react";
import { Navigate, useLocation } from "react-router-dom";

// Re-export auth hooks for convenience
export { useCurrentUser, useLogout, useInvalidateAuth } from "@/hooks/use-auth";

export function RequireAuth({ children }: { children: React.ReactNode }) {
	const location = useLocation();
	const { data: user, isLoading, isFetched } = useCurrentUser();

	const redirectTarget = useMemo(() => {
		const target = location.pathname + location.search;
		return `/login?redirect=${encodeURIComponent(target)}`;
	}, [location.pathname, location.search]);

	// Only show spinner on initial load, not on subsequent navigations
	// Return null to let the HTML preload splash remain visible during auth check
	if (isLoading && !isFetched) {
		return null;
	}

	// user is null means 401 response
	if (user === null) {
		return <Navigate to={redirectTarget} replace />;
	}

	return children;
}
