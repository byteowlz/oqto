"use client";

import { ModelQuickSwitcher } from "@/components/model-quick-switcher";
import { useSelectedChat } from "@/components/contexts";
import { useApp } from "@/hooks/use-app";
import { useCurrentUser } from "@/hooks/use-auth";
import { controlPlaneApiUrl, getAuthHeaders } from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import { useQuery } from "@tanstack/react-query";
import { Activity, Users } from "lucide-react";
import { useMemo } from "react";

// Types for status bar data
type HealthResponse = {
	status: string;
	version: string;
};

type AdminStats = {
	total_users: number;
	active_users: number;
	total_sessions: number;
	running_sessions: number;
};

// Fetch health/version info
async function fetchHealth(): Promise<HealthResponse> {
	const res = await fetch(controlPlaneApiUrl("/api/health"), {
		headers: getAuthHeaders(),
		credentials: "include",
	});
	if (!res.ok) {
		throw new Error("Failed to fetch health");
	}
	return res.json();
}

// Fetch admin stats (admin only)
async function fetchAdminStats(): Promise<AdminStats> {
	const res = await fetch(controlPlaneApiUrl("/api/admin/stats"), {
		headers: getAuthHeaders(),
		credentials: "include",
	});
	if (!res.ok) {
		throw new Error("Failed to fetch admin stats");
	}
	return res.json();
}

export function StatusBar() {
	const { data: user } = useCurrentUser();
	const { workspaceSessions, selectedChatSessionId, runnerSessionCount } = useApp();
	const { selectedChatFromHistory } = useSelectedChat();

	const isAdmin = (user?.role ?? "").toLowerCase() === "admin";

	// Fetch version from health endpoint
	const { data: health } = useQuery({
		queryKey: ["health"],
		queryFn: fetchHealth,
		staleTime: 5 * 60 * 1000, // 5 minutes
		gcTime: 30 * 60 * 1000,
		refetchOnWindowFocus: false,
	});

	// Fetch admin stats (only for admins)
	const { data: adminStats } = useQuery({
		queryKey: ["admin", "stats"],
		queryFn: fetchAdminStats,
		enabled: isAdmin,
		staleTime: 30 * 1000, // 30 seconds
		gcTime: 60 * 1000,
		refetchInterval: 30 * 1000, // Auto-refresh every 30s
		refetchOnWindowFocus: true,
	});

	// Count running sessions for current user
	const runningSessionCount = useMemo(() => {
		if (runnerSessionCount > 0) return runnerSessionCount;
		return workspaceSessions.filter((s) => s.status === "running").length;
	}, [runnerSessionCount, workspaceSessions]);

	// Get workspace path for model switcher from chat history or workspace sessions
	const workspacePath = useMemo(() => {
		if (selectedChatFromHistory?.workspace_path) {
			return selectedChatFromHistory.workspace_path;
		}
		// Fall back to first workspace session's path
		if (workspaceSessions?.length) {
			return workspaceSessions[0].workspace_path ?? null;
		}
		return null;
	}, [selectedChatFromHistory?.workspace_path, workspaceSessions]);

	return (
		<div
			className={cn(
				"flex items-center justify-between",
				"px-4 md:px-3",
				"bg-sidebar/80 border-t border-sidebar-border",
				"text-[10px] text-muted-foreground",
				"select-none",
				"h-10 md:h-6",
			)}
		>
			{/* Left side - user metrics */}
			<div className="flex items-center gap-2 md:gap-3 min-w-0">
				{/* Running sessions for current user */}
				<span
					className="flex items-center gap-1 shrink-0"
					title="Your running Pi sessions"
				>
					<Activity className="w-3 h-3" />
					<span className="font-mono">{runningSessionCount}</span>
				</span>

				{/* Current model - clickable to open quick switcher */}
				{selectedChatSessionId && (
					<ModelQuickSwitcherSafe
						sessionId={selectedChatSessionId}
						workspacePath={workspacePath}
						className="text-[10px] min-w-0"
					/>
				)}
			</div>

			{/* Right side - admin metrics + version */}
			<div className="flex items-center gap-2 md:gap-3 shrink-0">
				{/* Admin-only metrics */}
				{isAdmin && adminStats && (
					<>
						<span
							className="flex items-center gap-1"
							title="Active users / Total users"
						>
							<Users className="w-3 h-3" />
							<span className="font-mono">
								{adminStats.active_users}/{adminStats.total_users}
							</span>
						</span>
						<span
							className="flex items-center gap-1"
							title="Running sessions / Total sessions"
						>
							<Activity className="w-3 h-3" />
							<span className="font-mono">
								{adminStats.running_sessions}/{adminStats.total_sessions}
							</span>
						</span>
					</>
				)}

				{/* Version */}
				{health?.version && (
					<span className="font-mono opacity-60" title="Oqto version">
						v{health.version}
					</span>
				)}
			</div>
		</div>
	);
}

/** Error boundary wrapper so ModelQuickSwitcher crash doesn't take down the whole app. */
import { Component, type ErrorInfo, type ReactNode } from "react";

class ModelSwitcherErrorBoundary extends Component<
	{ children: ReactNode },
	{ error: Error | null }
> {
	state = { error: null as Error | null };

	static getDerivedStateFromError(error: Error) {
		return { error };
	}

	componentDidCatch(error: Error, info: ErrorInfo) {
		console.error("ModelQuickSwitcher crashed:", error, info.componentStack);
	}

	render() {
		if (this.state.error) {
			return (
				<span className="text-[10px] text-red-400" title={this.state.error.message}>
					model switcher error
				</span>
			);
		}
		return this.props.children;
	}
}

function ModelQuickSwitcherSafe(props: Parameters<typeof ModelQuickSwitcher>[0]) {
	return (
		<ModelSwitcherErrorBoundary>
			<ModelQuickSwitcher {...props} />
		</ModelSwitcherErrorBoundary>
	);
}
