import { useEffect } from "react";

interface AppRoute {
	id: string;
	routes?: string[];
}

interface UseAppShellRouteSyncInput {
	activeAppId: string;
	setActiveAppId: (appId: string) => void;
	matchedAppId: string | undefined;
	pathname: string;
	sessionsRoute: string | undefined;
	virtualApps: Set<string>;
	apps: AppRoute[];
	navigate: (path: string, options?: { replace?: boolean }) => void;
}

export function useAppShellRouteSync({
	activeAppId,
	setActiveAppId,
	matchedAppId,
	pathname,
	sessionsRoute,
	virtualApps,
	apps,
	navigate,
}: UseAppShellRouteSyncInput): void {
	// useeffect-guardrail: allow - route->app state synchronization and canonicalization
	useEffect(() => {
		if (matchedAppId && matchedAppId !== activeAppId) {
			if (matchedAppId === "sessions" && virtualApps.has(activeAppId)) return;
			setActiveAppId(matchedAppId);
			if (virtualApps.has(matchedAppId) && sessionsRoute) {
				navigate(sessionsRoute, { replace: true });
			}
			return;
		}
		if (!matchedAppId && pathname === "/" && sessionsRoute) {
			navigate(sessionsRoute, { replace: true });
			return;
		}

		if (activeAppId !== "sessions") return;
		const activeRoute = apps.find((app) => app.id === activeAppId)?.routes?.[0];
		if (!activeRoute || matchedAppId) return;
		const isMatch =
			pathname === activeRoute || pathname.startsWith(`${activeRoute}/`);
		if (!isMatch) navigate(activeRoute, { replace: true });
	}, [
		activeAppId,
		apps,
		matchedAppId,
		navigate,
		pathname,
		sessionsRoute,
		setActiveAppId,
		virtualApps,
	]);
}
