"use client";

import { ProviderIcon } from "@/components/data-display";
import { ModelQuickSwitcher } from "@/components/model-quick-switcher";
import { useSelectedChat } from "@/components/contexts";
import { useApp } from "@/hooks/use-app";
import { useCurrentUser } from "@/hooks/use-auth";
import { controlPlaneApiUrl, getAuthHeaders } from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import { useQuery } from "@tanstack/react-query";
import { Activity, Users } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

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

// Shorten model string for display (format: "provider/model")
function shortenModelRef(modelRef: string): string {
	if (!modelRef) return "";

	const parts = modelRef.split("/");
	if (parts.length < 2) return modelRef;

	const provider = parts[0];
	const model = parts.slice(1).join("/");

	// Shorten common providers
	const shortProvider = provider
		.replace("anthropic", "anth")
		.replace("openai", "oai")
		.replace("google", "goog")
		.replace("bedrock", "bed");

	// Shorten model names
	const shortModel = model
		.replace("claude-", "c-")
		.replace("sonnet", "son")
		.replace("opus", "op")
		.replace("haiku", "hk")
		.replace("gpt-4", "g4")
		.replace("gpt-3.5", "g3.5")
		.replace("-turbo", "-t")
		.replace("-preview", "-p")
		.replace("-latest", "");

	return `${shortProvider}/${shortModel}`;
}

export function StatusBar() {
	const { data: user } = useCurrentUser();
	const { workspaceSessions, selectedChatSessionId, runnerSessionCount } = useApp();
	const { selectedChatFromHistory } = useSelectedChat();

	const isAdmin = (user?.role ?? "").toLowerCase() === "admin";

	// Track selected model from localStorage (same key used by sessions app)
	const [selectedModelRef, setSelectedModelRef] = useState<string | null>(null);

	// Storage key matches sessions app: octo:chatModel:${chatSessionId}
	const modelStorageKey = useMemo(() => {
		const activeSessionId = selectedChatSessionId;
		if (!activeSessionId) return null;
		return `octo:chatModel:${activeSessionId}`;
	}, [selectedChatSessionId]);

	// Fallback model from hstry session metadata
	const hstryModelRef = useMemo(() => {
		if (selectedChatFromHistory?.provider && selectedChatFromHistory?.model) {
			return `${selectedChatFromHistory.provider}/${selectedChatFromHistory.model}`;
		}
		return null;
	}, [selectedChatFromHistory?.provider, selectedChatFromHistory?.model]);

	// Read model from localStorage and listen for changes
	useEffect(() => {
		if (!modelStorageKey) {
			setSelectedModelRef(null);
			return;
		}

		// Initial read - localStorage takes priority, then hstry fallback
		const stored = localStorage.getItem(modelStorageKey);
		setSelectedModelRef(stored || hstryModelRef);

		// Listen for storage changes from other tabs
		const handleStorage = (e: StorageEvent) => {
			if (e.key === modelStorageKey) {
				setSelectedModelRef(e.newValue);
			}
		};

		// Poll for same-tab changes (localStorage events don't fire for same tab)
		const interval = setInterval(() => {
			const current = localStorage.getItem(modelStorageKey) || hstryModelRef;
			setSelectedModelRef((prev) => (current !== prev ? current : prev));
		}, 1000);

		window.addEventListener("storage", handleStorage);
		return () => {
			window.removeEventListener("storage", handleStorage);
			clearInterval(interval);
		};
	}, [modelStorageKey, hstryModelRef]);

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

	// Extract provider and model name from ref (format: "provider/model")
	const { provider, modelName, shortModel } = useMemo(() => {
		if (!selectedModelRef)
			return { provider: null, modelName: null, shortModel: null };
		const parts = selectedModelRef.split("/");
		if (parts.length < 2)
			return {
				provider: null,
				modelName: selectedModelRef,
				shortModel: selectedModelRef,
			};
		return {
			provider: parts[0],
			modelName: parts.slice(1).join("/"),
			shortModel: shortenModelRef(selectedModelRef),
		};
	}, [selectedModelRef]);

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
					<span className="font-mono opacity-60" title="Octo version">
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
