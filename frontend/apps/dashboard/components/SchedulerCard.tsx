import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { i18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { RefreshCw, Trash2 } from "lucide-react";
import { memo, useCallback, useState } from "react";

type ScheduleItem = {
	name: string;
	status: string;
	command: string;
	schedule: string;
	next_run?: string | null;
};

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

function humanizeCron(cron: string, _locale: "de" | "en"): string {
	const parts = cron.trim().split(/\s+/);
	if (parts.length !== 5) return cron;
	const [min, hour, dom, month, dow] = parts;

	const t = {
		runs: i18n.t("scheduler.runs"),
		every: i18n.t("scheduler.every"),
		at: i18n.t("scheduler.at"),
		minute: i18n.t("scheduler.minute"),
		minutes: i18n.t("scheduler.minutes"),
		hour: i18n.t("scheduler.hour"),
		hours: i18n.t("scheduler.hours"),
		day: i18n.t("scheduler.day"),
		days: i18n.t("scheduler.days"),
		daily: i18n.t("scheduler.daily"),
		weekly: i18n.t("scheduler.weekly"),
		monthly: i18n.t("scheduler.monthly"),
		yearly: i18n.t("scheduler.yearly"),
		on: i18n.t("scheduler.on"),
	};

	const dayNames = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
	const monthNames = [
		"Jan",
		"Feb",
		"Mar",
		"Apr",
		"May",
		"Jun",
		"Jul",
		"Aug",
		"Sep",
		"Oct",
		"Nov",
		"Dec",
	];

	const formatTime = (h: string, m: string) => {
		const hh = h.padStart(2, "0");
		const mm = m.padStart(2, "0");
		return `${hh}:${mm}`;
	};

	const formatList = (value: string) =>
		value
			.split(",")
			.map((item) => item.trim())
			.filter(Boolean)
			.join(", ");

	if (
		min === "*" &&
		hour === "*" &&
		dom === "*" &&
		month === "*" &&
		dow === "*"
	) {
		return `${t.runs} ${t.every} ${t.minute}`;
	}

	if (
		min.startsWith("*/") &&
		hour === "*" &&
		dom === "*" &&
		month === "*" &&
		dow === "*"
	) {
		const step = min.slice(2);
		return `${t.runs} ${t.every} ${step} ${t.minutes}`;
	}

	if (
		/^\d+$/.test(min) &&
		hour === "*" &&
		dom === "*" &&
		month === "*" &&
		dow === "*"
	) {
		return `${t.runs} ${t.at} ${min} ${t.minutes} ${t.every} ${t.hour}`;
	}

	if (
		min === "0" &&
		hour === "*" &&
		dom === "*" &&
		month === "*" &&
		dow === "*"
	) {
		return `${t.runs} ${t.every} ${t.hour}`;
	}

	if (
		/^\d+$/.test(min) &&
		/^\d+$/.test(hour) &&
		dom === "*" &&
		month === "*" &&
		dow === "*"
	) {
		return `${t.runs} ${t.daily} ${t.at} ${formatTime(hour, min)}`;
	}

	if (/^\d+$/.test(min) && /^\d+$/.test(hour) && dow !== "*") {
		const days = dow
			.split(",")
			.map((value) => dayNames[Number.parseInt(value, 10)] ?? value)
			.join(", ");
		return `${t.runs} ${t.weekly} ${t.on} ${days} ${t.at} ${formatTime(hour, min)}`;
	}

	if (/^\d+$/.test(min) && /^\d+$/.test(hour) && dom !== "*" && month !== "*") {
		const months = month
			.split(",")
			.map((value) => monthNames[Number.parseInt(value, 10) - 1] ?? value)
			.join(", ");
		return `${t.runs} ${t.yearly} ${t.on} ${months} ${dom} ${t.at} ${formatTime(hour, min)}`;
	}

	if (/^\d+$/.test(min) && /^\d+$/.test(hour) && dom !== "*") {
		return `${t.runs} ${t.monthly} ${t.on} ${dom} ${t.at} ${formatTime(hour, min)}`;
	}

	if (hour.includes(",") && /^\d+$/.test(min)) {
		const hours = formatList(hour)
			.split(", ")
			.map((h) => formatTime(h, min))
			.join(", ");
		return `${t.runs} ${t.daily} ${t.at} ${hours}`;
	}

	if (
		min === "0" &&
		hour.startsWith("*/") &&
		dom === "*" &&
		month === "*" &&
		dow === "*"
	) {
		const step = hour.slice(2);
		return `${t.runs} ${t.every} ${step} ${t.hours}`;
	}

	if (
		min.startsWith("*/") &&
		/^\d+$/.test(hour) &&
		dom === "*" &&
		month === "*" &&
		dow === "*"
	) {
		const step = min.slice(2);
		return `${t.runs} ${t.daily} ${t.at} ${formatTime(hour, "00")} ${t.every} ${step} ${t.minutes}`;
	}

	if (
		dom === "*" &&
		month === "*" &&
		dow !== "*" &&
		min === "*" &&
		hour === "*"
	) {
		const days = dow
			.split(",")
			.map((value) => dayNames[Number.parseInt(value, 10)] ?? value)
			.join(", ");
		return `${t.runs} ${t.weekly} ${t.on} ${days}`;
	}

	if (dom !== "*" && month === "*" && min === "0" && hour === "0") {
		return `${t.runs} ${t.monthly} ${t.on} ${dom}`;
	}

	if (dom !== "*" && month !== "*" && min === "0" && hour === "0") {
		const months = month
			.split(",")
			.map((value) => monthNames[Number.parseInt(value, 10) - 1] ?? value)
			.join(", ");
		return `${t.runs} ${t.yearly} ${t.on} ${months} ${dom}`;
	}

	return cron;
}

export type SchedulerCardProps = {
	title: string;
	reloadLabel: string;
	noTasksLabel: string;
	scheduleList: ScheduleItem[];
	scheduleStats: { total: number; enabled: number; disabled: number };
	schedulerError: string | null;
	schedulerLoading: boolean;
	locale: "de" | "en";
	onReload: () => void;
	onDelete?: (name: string) => Promise<void>;
};

export const SchedulerCard = memo(function SchedulerCard({
	title,
	reloadLabel,
	noTasksLabel,
	scheduleList,
	scheduleStats,
	schedulerError,
	schedulerLoading,
	locale,
	onReload,
	onDelete,
}: SchedulerCardProps) {
	const [deletingName, setDeletingName] = useState<string | null>(null);

	const handleDelete = useCallback(
		async (name: string) => {
			if (!onDelete) return;
			if (!window.confirm(`Delete scheduled job "${name}"?`)) return;
			setDeletingName(name);
			try {
				await onDelete(name);
			} finally {
				setDeletingName(null);
			}
		},
		[onDelete],
	);
	return (
		<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
			<CardHeader className="flex flex-row items-center justify-between">
				<div>
					<CardTitle>{title}</CardTitle>
					<CardDescription>
						{schedulerError
							? schedulerError
							: `${scheduleStats.enabled} enabled, ${scheduleStats.disabled} disabled`}
					</CardDescription>
				</div>
				<Button
					variant="outline"
					size="sm"
					onClick={onReload}
					disabled={schedulerLoading}
					className="gap-2"
				>
					<RefreshCw className="h-4 w-4" />
					{reloadLabel}
				</Button>
			</CardHeader>
			<CardContent className="flex-1 min-h-0 overflow-auto">
				{scheduleList.length === 0 ? (
					<div className="text-sm text-muted-foreground">{noTasksLabel}</div>
				) : (
					<div className="space-y-3">
						{scheduleList.slice(0, 6).map((schedule) => (
							<div
								key={schedule.name}
								className="flex flex-col md:flex-row md:items-center md:justify-between gap-2 border-b border-border/40 pb-3 last:border-b-0 last:pb-0"
							>
								<div className="min-w-0">
									<div className="flex items-center gap-2">
										<p className="font-medium text-sm truncate">
											{schedule.name}
										</p>
										<StatusPill status={schedule.status} />
									</div>
									<p className="text-xs text-muted-foreground truncate">
										{schedule.command}
									</p>
								</div>
								<div className="flex items-center gap-2">
									<div className="text-xs text-muted-foreground text-right">
										{schedule.schedule.trim().split(/\s+/).length === 5 ? (
											<>
												<div>{humanizeCron(schedule.schedule, locale)}</div>
												<div className="opacity-70">{schedule.schedule}</div>
											</>
										) : (
											<div>Once: {schedule.schedule}</div>
										)}
										{schedule.next_run && <div>Next: {schedule.next_run}</div>}
									</div>
									{onDelete && (
										<Button
											variant="ghost"
											size="sm"
											className="h-7 w-7 p-0 text-muted-foreground hover:text-destructive shrink-0"
											onClick={() => handleDelete(schedule.name)}
											disabled={deletingName === schedule.name}
											title={`Delete "${schedule.name}"`}
										>
											<Trash2 className="h-3.5 w-3.5" />
										</Button>
									)}
								</div>
							</div>
						))}
					</div>
				)}
			</CardContent>
		</Card>
	);
});
