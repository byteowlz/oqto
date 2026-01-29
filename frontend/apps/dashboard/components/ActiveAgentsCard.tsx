import { Badge } from "@/components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import type { OpenCodeAgent } from "@/lib/opencode-client";
import { formatSessionDate } from "@/lib/session-utils";
import type { WorkspaceSession } from "@/lib/workspace-session";
import { cn } from "@/lib/utils";
import { Bot } from "lucide-react";
import { memo } from "react";
import type { OpenCodeSession } from "@/lib/opencode-client";

function StatusPill({ status }: { status: string }) {
	const normalized = status.toLowerCase();
	const classes =
		normalized === "enabled" || normalized === "running"
			? "bg-emerald-500/10 text-emerald-300 border-emerald-500/40"
			: normalized === "disabled" || normalized === "stopped"
				? "bg-amber-500/10 text-amber-300 border-amber-500/40"
				: normalized === "failed"
					? "bg-rose-500/10 text-rose-300 border-rose-500/40"
					: "bg-muted/60 text-muted-foreground border-border";
	return (
		<span className={cn("text-xs px-2 py-1 rounded-full border", classes)}>
			{status}
		</span>
	);
}

function formatDateTime(value?: string | null): string {
	if (!value) return "";
	const date = new Date(value);
	if (Number.isNaN(date.getTime())) return value;
	return formatSessionDate(date.getTime());
}

export type ActiveAgentsCardProps = {
	title: string;
	noAgentsLabel: string;
	runningSessions: WorkspaceSession[];
	busyChatSessions: OpenCodeSession[];
	agents: OpenCodeAgent[];
};

export const ActiveAgentsCard = memo(function ActiveAgentsCard({
	title,
	noAgentsLabel,
	runningSessions,
	busyChatSessions,
	agents,
}: ActiveAgentsCardProps) {
	return (
		<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
			<CardHeader>
				<CardTitle>{title}</CardTitle>
				<CardDescription>
					{runningSessions.length} running containers, {agents.length} agent
					profiles
				</CardDescription>
			</CardHeader>
			<CardContent className="flex-1 min-h-0 overflow-auto space-y-4">
				{runningSessions.length === 0 ? (
					<div className="text-sm text-muted-foreground">{noAgentsLabel}</div>
				) : (
					<div className="space-y-3">
						{runningSessions.map((session) => (
							<div
								key={session.id}
								className="flex flex-col md:flex-row md:items-center md:justify-between gap-2 border-b border-border/40 pb-3 last:border-b-0 last:pb-0"
							>
								<div className="min-w-0">
									<div className="flex items-center gap-2">
										<Bot className="h-4 w-4 text-primary" />
										<p className="font-medium text-sm truncate">
											{session.persona?.name ?? session.container_name}
										</p>
										<StatusPill status={session.status} />
									</div>
									<p className="text-xs text-muted-foreground truncate">
										{session.workspace_path}
									</p>
								</div>
								<div className="text-xs text-muted-foreground text-right">
									Started {formatDateTime(session.started_at)}
								</div>
							</div>
						))}
					</div>
				)}

				<div>
					<div className="flex items-center justify-between mb-2">
						<p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
							Busy chats
						</p>
						<Badge variant="secondary">{busyChatSessions.length}</Badge>
					</div>
					{busyChatSessions.length === 0 ? (
						<p className="text-sm text-muted-foreground">No chats busy.</p>
					) : (
						<div className="flex flex-wrap gap-2">
							{busyChatSessions.map((session) => (
								<span
									key={session.id}
									className="text-xs px-2 py-1 rounded-full bg-muted border border-border"
								>
									{session.title || session.id}
								</span>
							))}
						</div>
					)}
				</div>
			</CardContent>
		</Card>
	);
});
