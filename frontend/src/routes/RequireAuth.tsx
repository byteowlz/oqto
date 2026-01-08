import { useCurrentUser } from "@/hooks/use-auth";
import { useTheme } from "next-themes";
import type React from "react";
import { useMemo } from "react";
import { Navigate, useLocation } from "react-router-dom";

export function RequireAuth({ children }: { children: React.ReactNode }) {
	const location = useLocation();
	const { data: user, isLoading, isFetched } = useCurrentUser();
	const { resolvedTheme } = useTheme();
	const isDark = resolvedTheme === "dark";

	const redirectTarget = useMemo(() => {
		const target = location.pathname + location.search;
		return `/login?redirect=${encodeURIComponent(target)}`;
	}, [location.pathname, location.search]);

	// Only show spinner on initial load, not on subsequent navigations
	if (isLoading && !isFetched) {
		return (
			<div className="min-h-screen flex items-center justify-center bg-background text-foreground">
				<div className="flex flex-col items-center gap-4">
					<img
						src={
							isDark ? "/octo_logo_new_white.png" : "/octo_logo_new_black.png"
						}
						alt="OCTO"
						width={120}
						height={48}
						className="h-12 w-auto object-contain animate-pulse"
					/>
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
