"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import {
	type AdminSession,
	type SessionContainerStats,
	useAdminSessions,
	useForceStopSession,
} from "@/hooks/use-admin";
import {
	AlertTriangle,
	Box,
	Clock,
	Container,
	Cpu,
	FolderOpen,
	HardDrive,
	RefreshCw,
	Square,
	User,
} from "lucide-react";
import { useState } from "react";

type SessionWithStats = AdminSession & {
	containerStats?: SessionContainerStats;
};

function StatusBadge({ status }: { status: string }) {
	const variants: Record<
		string,
		"default" | "secondary" | "destructive" | "outline"
	> = {
		running: "default",
		starting: "secondary",
		pending: "secondary",
		stopping: "secondary",
		stopped: "outline",
		failed: "destructive",
	};

	return (
		<Badge
			variant={variants[status] ?? "outline"}
			className="uppercase text-[10px]"
		>
			{status}
		</Badge>
	);
}

function RuntimeBadge({ mode }: { mode: string }) {
	return (
		<Badge variant="outline" className="text-[10px] gap-1">
			{mode === "container" ? (
				<Container className="w-3 h-3" />
			) : (
				<Box className="w-3 h-3" />
			)}
			{mode}
		</Badge>
	);
}

function formatDuration(startedAt: string | null): string {
	if (!startedAt) return "-";
	const start = new Date(startedAt);
	const now = new Date();
	const diffMs = now.getTime() - start.getTime();
	const diffSecs = Math.floor(diffMs / 1000);
	const hours = Math.floor(diffSecs / 3600);
	const minutes = Math.floor((diffSecs % 3600) / 60);
	const seconds = diffSecs % 60;

	if (hours > 0) {
		return `${hours}h ${minutes}m`;
	}
	if (minutes > 0) {
		return `${minutes}m ${seconds}s`;
	}
	return `${seconds}s`;
}

function formatWorkspacePath(path: string): string {
	// Extract just the last part of the path for display
	const parts = path.split("/");
	return parts[parts.length - 1] || path;
}

function SessionRow({
	session,
	containerStats,
	onKill,
	isKilling,
}: {
	session: AdminSession;
	containerStats?: SessionContainerStats;
	onKill: (id: string) => void;
	isKilling: boolean;
}) {
	const isActive =
		session.status === "running" || session.status === "starting";

	return (
		<tr className="border-b border-border hover:bg-muted/30 transition">
			<td className="py-3 px-4">
				<div className="flex flex-col gap-1">
					<code className="text-xs text-foreground">
						{session.id.slice(0, 8)}
					</code>
					<span className="text-[10px] text-muted-foreground">
						{session.container_name}
					</span>
				</div>
			</td>
			<td className="py-3 px-4">
				<div className="flex items-center gap-2">
					<User className="w-3 h-3 text-muted-foreground" />
					<span className="text-sm text-muted-foreground truncate max-w-[150px]">
						{session.user_id.slice(0, 8)}...
					</span>
				</div>
			</td>
			<td className="py-3 px-4">
				<TooltipProvider>
					<Tooltip>
						<TooltipTrigger asChild>
							<div className="flex items-center gap-2">
								<FolderOpen className="w-3 h-3 text-muted-foreground" />
								<span className="text-sm text-muted-foreground truncate max-w-[150px]">
									{formatWorkspacePath(session.workspace_path)}
								</span>
							</div>
						</TooltipTrigger>
						<TooltipContent>
							<p className="font-mono text-xs">{session.workspace_path}</p>
						</TooltipContent>
					</Tooltip>
				</TooltipProvider>
			</td>
			<td className="py-3 px-4">
				<div className="flex items-center gap-2">
					<StatusBadge status={session.status} />
					<RuntimeBadge mode={session.runtime_mode} />
				</div>
			</td>
			<td className="py-3 px-4">
				<div className="flex items-center gap-1 text-sm font-mono text-muted-foreground">
					<Clock className="w-3 h-3" />
					{formatDuration(session.started_at)}
				</div>
			</td>
			<td className="py-3 px-4">
				{containerStats ? (
					<div className="flex flex-col gap-1 text-xs font-mono">
						<div className="flex items-center gap-1 text-muted-foreground">
							<Cpu className="w-3 h-3" />
							{containerStats.stats.cpu_percent}
						</div>
						<div className="flex items-center gap-1 text-muted-foreground">
							<HardDrive className="w-3 h-3" />
							{containerStats.stats.mem_usage}
						</div>
					</div>
				) : session.runtime_mode === "local" ? (
					<span className="text-xs text-muted-foreground">N/A (local)</span>
				) : (
					<span className="text-xs text-muted-foreground">-</span>
				)}
			</td>
			<td className="py-3 px-4">
				{isActive && (
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={() => onKill(session.id)}
						disabled={isKilling}
						className="border-destructive text-destructive hover:bg-destructive/10 bg-transparent h-7 text-xs"
					>
						{isKilling ? (
							<RefreshCw className="w-3 h-3 animate-spin" />
						) : (
							<>
								<Square className="w-3 h-3 mr-1" />
								Kill
							</>
						)}
					</Button>
				)}
				{session.status === "failed" && session.error_message && (
					<TooltipProvider>
						<Tooltip>
							<TooltipTrigger asChild>
								<div className="flex items-center gap-1 text-destructive">
									<AlertTriangle className="w-4 h-4" />
								</div>
							</TooltipTrigger>
							<TooltipContent side="left" className="max-w-xs">
								<p className="text-xs">{session.error_message}</p>
							</TooltipContent>
						</Tooltip>
					</TooltipProvider>
				)}
			</td>
		</tr>
	);
}

function MobileSessionCard({
	session,
	containerStats,
	onKill,
	isKilling,
}: {
	session: AdminSession;
	containerStats?: SessionContainerStats;
	onKill: (id: string) => void;
	isKilling: boolean;
}) {
	const isActive =
		session.status === "running" || session.status === "starting";

	return (
		<div className="border border-border p-3 space-y-2">
			<div className="flex items-start justify-between gap-2">
				<div className="min-w-0 space-y-1">
					<code className="text-xs text-foreground block">
						{session.id.slice(0, 8)}
					</code>
					<span className="text-xs text-muted-foreground block truncate">
						{session.container_name}
					</span>
				</div>
				{isActive && (
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={() => onKill(session.id)}
						disabled={isKilling}
						className="border-destructive text-destructive hover:bg-destructive/10 bg-transparent h-7 text-xs shrink-0"
					>
						{isKilling ? (
							<RefreshCw className="w-3 h-3 animate-spin" />
						) : (
							"Kill"
						)}
					</Button>
				)}
			</div>

			<div className="flex flex-wrap gap-2 items-center">
				<StatusBadge status={session.status} />
				<RuntimeBadge mode={session.runtime_mode} />
			</div>

			<div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
				<span className="flex items-center gap-1">
					<User className="w-3 h-3" />
					{session.user_id.slice(0, 8)}...
				</span>
				<span className="flex items-center gap-1">
					<Clock className="w-3 h-3" />
					{formatDuration(session.started_at)}
				</span>
			</div>

			<div className="text-xs text-muted-foreground truncate">
				<FolderOpen className="w-3 h-3 inline mr-1" />
				{session.workspace_path}
			</div>

			{containerStats && (
				<div className="flex gap-4 text-xs font-mono text-muted-foreground">
					<span>
						<Cpu className="w-3 h-3 inline mr-1" />
						{containerStats.stats.cpu_percent}
					</span>
					<span>
						<HardDrive className="w-3 h-3 inline mr-1" />
						{containerStats.stats.mem_usage}
					</span>
				</div>
			)}

			{session.status === "failed" && session.error_message && (
				<div className="text-xs text-destructive flex items-start gap-1">
					<AlertTriangle className="w-3 h-3 mt-0.5 shrink-0" />
					<span className="truncate">{session.error_message}</span>
				</div>
			)}
		</div>
	);
}

export function SessionsPanel({
	containerStats,
}: {
	containerStats?: SessionContainerStats[];
}) {
	const { data: sessions, isLoading, error, refetch } = useAdminSessions();
	const forceStopMutation = useForceStopSession();
	const [killingIds, setKillingIds] = useState<Set<string>>(new Set());

	const handleKill = async (sessionId: string) => {
		setKillingIds((prev) => new Set([...prev, sessionId]));
		try {
			await forceStopMutation.mutateAsync(sessionId);
		} finally {
			setKillingIds((prev) => {
				const next = new Set(prev);
				next.delete(sessionId);
				return next;
			});
		}
	};

	// Create a map of container stats by session ID
	const statsMap = new Map(containerStats?.map((s) => [s.session_id, s]) ?? []);

	if (error) {
		return (
			<div className="bg-card border border-border">
				<div className="border-b border-border px-3 md:px-4 py-2 md:py-3 flex items-center justify-between">
					<h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">
						SESSIONS
					</h2>
					<Button
						variant="ghost"
						size="sm"
						onClick={() => refetch()}
						className="h-7"
					>
						<RefreshCw className="w-3 h-3" />
					</Button>
				</div>
				<div className="p-4 text-sm text-destructive flex items-center gap-2">
					<AlertTriangle className="w-4 h-4" />
					Failed to load sessions: {error.message}
				</div>
			</div>
		);
	}

	const activeSessions = sessions?.filter(
		(s) => s.status === "running" || s.status === "starting",
	);
	const inactiveSessions = sessions?.filter(
		(s) => s.status !== "running" && s.status !== "starting",
	);

	return (
		<div className="bg-card border border-border">
			<div className="border-b border-border px-3 md:px-4 py-2 md:py-3 flex items-center justify-between">
				<div className="flex items-center gap-3">
					<h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">
						SESSIONS
					</h2>
					{sessions && (
						<Badge variant="secondary" className="text-[10px]">
							{activeSessions?.length ?? 0} active / {sessions.length} total
						</Badge>
					)}
				</div>
				<Button
					variant="ghost"
					size="sm"
					onClick={() => refetch()}
					className="h-7"
				>
					<RefreshCw className="w-3 h-3" />
				</Button>
			</div>

			{isLoading ? (
				<div className="p-4 space-y-3">
					<Skeleton className="h-12 w-full" />
					<Skeleton className="h-12 w-full" />
					<Skeleton className="h-12 w-full" />
				</div>
			) : sessions && sessions.length > 0 ? (
				<>
					{/* Mobile: Card Layout */}
					<div className="md:hidden p-3 space-y-3">
						{sessions.map((session) => (
							<MobileSessionCard
								key={session.id}
								session={session}
								containerStats={statsMap.get(session.id)}
								onKill={handleKill}
								isKilling={killingIds.has(session.id)}
							/>
						))}
					</div>

					{/* Desktop: Table Layout */}
					<div className="hidden md:block p-4 overflow-x-auto">
						<table className="w-full min-w-[800px]">
							<thead>
								<tr className="border-b border-border">
									<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
										SESSION
									</th>
									<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
										USER
									</th>
									<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
										WORKSPACE
									</th>
									<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
										STATUS
									</th>
									<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
										UPTIME
									</th>
									<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
										RESOURCES
									</th>
									<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
										ACTIONS
									</th>
								</tr>
							</thead>
							<tbody>
								{sessions.map((session) => (
									<SessionRow
										key={session.id}
										session={session}
										containerStats={statsMap.get(session.id)}
										onKill={handleKill}
										isKilling={killingIds.has(session.id)}
									/>
								))}
							</tbody>
						</table>
					</div>
				</>
			) : (
				<div className="p-8 text-center text-sm text-muted-foreground">
					No sessions found
				</div>
			)}
		</div>
	);
}
