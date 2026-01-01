import { Spinner } from "@/components/ui/spinner";
import type React from "react";
import { useEffect, useMemo, useState } from "react";
import { Navigate, useLocation } from "react-router-dom";

export function RequireAuth({ children }: { children: React.ReactNode }) {
	const location = useLocation();
	const [status, setStatus] = useState<"checking" | "authed" | "unauth">(
		"checking",
	);

	useEffect(() => {
		let cancelled = false;
		const pathAtRequest = location.pathname;

		const checkAuth = async () => {
			try {
				const response = await fetch("/api/me", { credentials: "include" });
				if (cancelled || pathAtRequest !== location.pathname) return;
				if (response.status === 401) {
					setStatus("unauth");
					return;
				}
				setStatus("authed");
			} catch {
				if (cancelled || pathAtRequest !== location.pathname) return;
				// If the API is not reachable, allow the app to render.
				setStatus("authed");
			}
		};

		setStatus("checking");
		void checkAuth();

		return () => {
			cancelled = true;
		};
	}, [location.pathname]);

	const redirectTarget = useMemo(() => {
		const target = location.pathname + location.search;
		return `/login?redirect=${encodeURIComponent(target)}`;
	}, [location.pathname, location.search]);

	if (status === "checking") {
		return (
			<div className="min-h-screen flex items-center justify-center bg-background text-foreground">
				<div className="flex items-center gap-2 text-sm text-muted-foreground">
					<Spinner className="size-4" />
					<span>Checking session...</span>
				</div>
			</div>
		);
	}

	if (status === "unauth") {
		return <Navigate to={redirectTarget} replace />;
	}

	return children;
}
