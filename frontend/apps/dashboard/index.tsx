"use client";

import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { useApp } from "@/hooks/use-app";
import {
	controlPlaneApiUrl,
	getCodexBarUsage,
	fetchFeed,
	getAuthHeaders,
	getSchedulerOverview,
	type SchedulerOverview,
	type CodexBarUsagePayload,
} from "@/lib/control-plane-client";
import { type OpenCodeAgent, fetchAgents } from "@/lib/opencode-client";
import { formatSessionDate } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	Activity,
	Bot,
	CalendarClock,
	CheckCircle2,
	Flame,
	ListTodo,
	RefreshCw,
	Rss,
	Sparkles,
	X,
} from "lucide-react";
import {
	memo,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";

type TrxIssue = {
	id: string;
	title: string;
	status: string;
	priority: number;
	issue_type: string;
	updated_at: string;
};

type FeedItem = {
	id: string;
	title: string;
	link?: string;
	date?: string;
};

type FeedState = {
	title: string;
	items: FeedItem[];
	loading: boolean;
	error?: string;
};

type CodexBarState = {
	available: boolean;
	loading: boolean;
	error?: string;
	payload: CodexBarUsagePayload[];
};

const FEED_STORAGE_KEY = "octo:dashboardFeeds";

function createId(): string {
	if (typeof crypto !== "undefined" && crypto.randomUUID) {
		return crypto.randomUUID();
	}
	return Math.random().toString(36).slice(2);
}

function formatDateTime(value?: string | null): string {
	if (!value) return "";
	const date = new Date(value);
	if (Number.isNaN(date.getTime())) return value;
	return formatSessionDate(date.getTime());
}

function humanizeCron(cron: string, locale: "de" | "en"): string {
	const parts = cron.trim().split(/\s+/);
	if (parts.length !== 5) return cron;
	const [min, hour, dom, month, dow] = parts;

	const t = {
		runs: locale === "de" ? "Laeuft" : "Runs",
		every: locale === "de" ? "jede" : "every",
		at: locale === "de" ? "um" : "at",
		minute: locale === "de" ? "Minute" : "minute",
		minutes: locale === "de" ? "Minuten" : "minutes",
		hour: locale === "de" ? "Stunde" : "hour",
		hours: locale === "de" ? "Stunden" : "hours",
		day: locale === "de" ? "Tag" : "day",
		days: locale === "de" ? "Tage" : "days",
		daily: locale === "de" ? "taeglich" : "daily",
		weekly: locale === "de" ? "woechentlich" : "weekly",
		monthly: locale === "de" ? "monatlich" : "monthly",
		yearly: locale === "de" ? "jaehrlich" : "yearly",
		on: locale === "de" ? "am" : "on",
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

	if (min === "*" && hour === "*" && dom === "*" && month === "*" && dow === "*") {
		return `${t.runs} ${t.every} ${t.minute}`;
	}

	if (min.startsWith("*/") && hour === "*" && dom === "*" && month === "*" && dow === "*") {
		const step = min.slice(2);
		return `${t.runs} ${t.every} ${step} ${t.minutes}`;
	}

	if (/^\d+$/.test(min) && hour === "*" && dom === "*" && month === "*" && dow === "*") {
		return `${t.runs} ${t.at} ${min} ${t.minutes} ${t.every} ${t.hour}`;
	}

	if (min === "0" && hour === "*" && dom === "*" && month === "*" && dow === "*") {
		return `${t.runs} ${t.every} ${t.hour}`;
	}

	if (/^\d+$/.test(min) && /^\d+$/.test(hour) && dom === "*" && month === "*" && dow === "*") {
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

	if (min === "0" && hour.startsWith("*/") && dom === "*" && month === "*" && dow === "*") {
		const step = hour.slice(2);
		return `${t.runs} ${t.every} ${step} ${t.hours}`;
	}

	if (min.startsWith("*/") && /^\d+$/.test(hour) && dom === "*" && month === "*" && dow === "*") {
		const step = min.slice(2);
		return `${t.runs} ${t.daily} ${t.at} ${formatTime(hour, "00")} ${t.every} ${step} ${t.minutes}`;
	}

	if (dom === "*" && month === "*" && dow !== "*" && min === "*" && hour === "*") {
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

function parseFeedXml(xml: string): FeedState {
	const parser = new DOMParser();
	const doc = parser.parseFromString(xml, "application/xml");

	const atomFeed = doc.querySelector("feed");
	const rssFeed = doc.querySelector("rss, RDF");

	if (atomFeed) {
		const title = atomFeed.querySelector("title")?.textContent?.trim() ??
			"Atom Feed";
		const entries = Array.from(atomFeed.querySelectorAll("entry")).map(
			(entry) => {
				const id =
					entry.querySelector("id")?.textContent?.trim() ??
					createId();
				const link =
					entry
						.querySelector("link[rel='alternate']")
						?.getAttribute("href") ??
					entry.querySelector("link")?.getAttribute("href") ??
					undefined;
				const date =
					entry.querySelector("updated")?.textContent?.trim() ??
					entry.querySelector("published")?.textContent?.trim() ??
					undefined;
				return {
					id,
					title:
						entry.querySelector("title")?.textContent?.trim() ?? "Untitled",
					link,
					date,
				};
			},
		);
		return {
			title,
			items: entries,
			loading: false,
		};
	}

	if (rssFeed) {
		const channel = doc.querySelector("channel");
		const title =
			channel?.querySelector("title")?.textContent?.trim() ?? "RSS Feed";
		const items = Array.from(doc.querySelectorAll("item")).map((item) => {
			const guid = item.querySelector("guid")?.textContent?.trim();
			const link = item.querySelector("link")?.textContent?.trim();
			const titleText = item.querySelector("title")?.textContent?.trim();
			return {
				id: guid || link || createId(),
				title: titleText || "Untitled",
				link: link || undefined,
				date: item.querySelector("pubDate")?.textContent?.trim() ?? undefined,
			};
		});
		return {
			title,
			items,
			loading: false,
		};
	}

	return {
		title: "Feed",
		items: [],
		loading: false,
		error: "Unsupported feed format",
	};
}

async function fetchTrxIssues(workspacePath: string): Promise<TrxIssue[]> {
	const res = await fetch(
		controlPlaneApiUrl(
			`/api/workspace/trx/issues?workspace_path=${encodeURIComponent(
				workspacePath,
			)}`,
		),
		{
			headers: {
				...getAuthHeaders(),
			},
			credentials: "include",
		},
	);
	if (res.status === 404) {
		return [];
	}
	if (!res.ok) {
		const text = await res.text();
		throw new Error(text || "Failed to fetch TRX issues");
	}
	return res.json();
}

const StatCard = memo(function StatCard({
	label,
	value,
	subValue,
	Icon,
	accent,
}: {
	label: string;
	value: string | number;
	subValue?: string;
	Icon: React.ElementType;
	accent?: string;
}) {
	return (
		<Card className="border border-border/60 bg-card/80">
			<CardContent className="p-4 flex items-center justify-between">
				<div className="min-w-0">
					<p className="text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
						{label}
					</p>
					<p className="text-2xl font-bold mt-1 text-foreground font-mono">
						{value}
					</p>
					{subValue && (
						<p className="text-xs text-muted-foreground mt-1">{subValue}</p>
					)}
				</div>
				<div
					className={cn(
						"p-2 rounded-lg border",
						accent ?? "border-primary/20 text-primary",
					)}
				>
					<Icon className="h-6 w-6" />
				</div>
			</CardContent>
		</Card>
	);
});

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

export function DashboardApp() {
	const {
		locale,
		workspaceSessions,
		opencodeSessions,
		busySessions,
		opencodeBaseUrl,
		opencodeDirectory,
		selectedWorkspaceSession,
	} = useApp();
	const [scheduler, setScheduler] = useState<SchedulerOverview | null>(null);
	const [schedulerError, setSchedulerError] = useState<string | null>(null);
	const [schedulerLoading, setSchedulerLoading] = useState(false);
	const [agents, setAgents] = useState<OpenCodeAgent[]>([]);
	const [trxIssues, setTrxIssues] = useState<TrxIssue[]>([]);
	const [trxError, setTrxError] = useState<string | null>(null);
	const [trxLoading, setTrxLoading] = useState(false);
	const [feedUrls, setFeedUrls] = useState<string[]>(() => {
		if (typeof window === "undefined") return [];
		try {
			const stored = localStorage.getItem(FEED_STORAGE_KEY);
			return stored ? (JSON.parse(stored) as string[]) : [];
		} catch {
			return [];
		}
	});
	const [feedInput, setFeedInput] = useState("");
	const [feeds, setFeeds] = useState<Record<string, FeedState>>({});
	const mountedRef = useRef(true);
	const [codexbar, setCodexbar] = useState<CodexBarState>({
		available: false,
		loading: false,
		payload: [],
	});

	const workspacePath =
		selectedWorkspaceSession?.workspace_path ?? opencodeDirectory ?? ".";

	const copy = useMemo(
		() => ({
			de: {
				title: "Dashboard",
				subtitle: "Arbeitsstatus, Scheduler, TRX und Feeds auf einen Blick.",
				stats: "Statusuebersicht",
				scheduler: "Geplante Tasks",
				workingAgents: "Aktive Agents",
				trx: "TRX Ueberblick",
				feeds: "Feeds",
				addFeed: "Feed hinzufuegen",
				reload: "Aktualisieren",
				noTasks: "Keine Schedules gefunden.",
				noAgents: "Keine aktiven Agents.",
				noTrx: "Keine TRX-Issues.",
				noFeeds: "Noch keine Feeds.",
			},
			en: {
				title: "Dashboard",
				subtitle: "Workspace status, scheduler, TRX, and feeds at a glance.",
				stats: "Status Overview",
				scheduler: "Scheduled Tasks",
				workingAgents: "Working Agents",
				trx: "TRX Overview",
				feeds: "Feeds",
				addFeed: "Add feed",
				reload: "Refresh",
				noTasks: "No schedules found.",
				noAgents: "No active agents.",
				noTrx: "No TRX issues yet.",
				noFeeds: "No feeds added yet.",
			},
		}),
		[],
	);
	const t = copy[locale];

	const runningSessions = useMemo(
		() => workspaceSessions.filter((session) => session.status === "running"),
		[workspaceSessions],
	);

	const busyChatSessions = useMemo(() => {
		if (!busySessions.size) return [];
		return opencodeSessions.filter((session) => busySessions.has(session.id));
	}, [busySessions, opencodeSessions]);

	const trxStats = useMemo(() => {
		const open = trxIssues.filter((issue) => issue.status !== "closed");
		const inProgress = trxIssues.filter(
			(issue) => issue.status === "in_progress",
		);
		const blocked = trxIssues.filter((issue) => issue.status === "blocked");
		return {
			total: trxIssues.length,
			open: open.length,
			inProgress: inProgress.length,
			blocked: blocked.length,
		};
	}, [trxIssues]);

	const scheduleStats = scheduler?.stats ?? {
		total: 0,
		enabled: 0,
		disabled: 0,
	};

	const handleLoadScheduler = useCallback(async () => {
		setSchedulerLoading(true);
		setSchedulerError(null);
		try {
			const data = await getSchedulerOverview();
			setScheduler(data);
		} catch (err) {
			console.error("Failed to load scheduler overview:", err);
			setSchedulerError(err instanceof Error ? err.message : "Unknown error");
		} finally {
			setSchedulerLoading(false);
		}
	}, []);

	const handleLoadAgents = useCallback(async () => {
		if (!opencodeBaseUrl) return;
		try {
			const list = await fetchAgents(opencodeBaseUrl, {
				directory: opencodeDirectory,
			});
			setAgents(list);
		} catch (err) {
			console.error("Failed to fetch agents:", err);
			setAgents([]);
		}
	}, [opencodeBaseUrl, opencodeDirectory]);

	const handleLoadTrx = useCallback(async () => {
		if (!workspacePath) return;
		setTrxLoading(true);
		setTrxError(null);
		try {
			const data = await fetchTrxIssues(workspacePath);
			setTrxIssues(data);
		} catch (err) {
			console.error("Failed to fetch TRX issues:", err);
			setTrxError(err instanceof Error ? err.message : "Unknown error");
		} finally {
			setTrxLoading(false);
		}
	}, [workspacePath]);

	const handleAddFeed = useCallback(() => {
		const trimmed = feedInput.trim();
		if (!trimmed) return;
		if (feedUrls.includes(trimmed)) {
			setFeedInput("");
			return;
		}
		const next = [trimmed, ...feedUrls].slice(0, 8);
		setFeedUrls(next);
		setFeedInput("");
	}, [feedInput, feedUrls]);

	const handleRemoveFeed = useCallback(
		(url: string) => {
			const next = feedUrls.filter((entry) => entry !== url);
			setFeedUrls(next);
			setFeeds((prev) => {
				const updated = { ...prev };
				delete updated[url];
				return updated;
			});
		},
		[feedUrls],
	);

	const loadFeed = useCallback(async (url: string) => {
		setFeeds((prev) => ({
			...prev,
			[url]: {
				title: prev[url]?.title ?? "Feed",
				items: prev[url]?.items ?? [],
				loading: true,
			},
		}));
		try {
			const response = await fetchFeed(url);
			const parsed = parseFeedXml(response.content);
			if (!mountedRef.current) return;
			setFeeds((prev) => ({
				...prev,
				[url]: { ...parsed, loading: false },
			}));
		} catch (err) {
			if (!mountedRef.current) return;
			setFeeds((prev) => ({
				...prev,
				[url]: {
					title: prev[url]?.title ?? "Feed",
					items: prev[url]?.items ?? [],
					loading: false,
					error: err instanceof Error ? err.message : "Failed to load feed",
				},
			}));
		}
	}, []);

	const handleRefreshFeeds = useCallback(() => {
		feedUrls.forEach((url) => loadFeed(url));
	}, [feedUrls, loadFeed]);

	const handleLoadCodexbar = useCallback(async () => {
		setCodexbar((prev) => ({ ...prev, loading: true, error: undefined }));
		try {
			const payload = await getCodexBarUsage();
			if (!payload) {
				setCodexbar({ available: false, loading: false, payload: [] });
				return;
			}
			setCodexbar({ available: true, loading: false, payload });
		} catch (err) {
			setCodexbar((prev) => ({
				...prev,
				available: true,
				loading: false,
				error: err instanceof Error ? err.message : "Failed to load",
			}));
		}
	}, []);

	useEffect(() => {
		mountedRef.current = true;
		return () => {
			mountedRef.current = false;
		};
	}, []);

	useEffect(() => {
		if (typeof window === "undefined") return;
		try {
			localStorage.setItem(FEED_STORAGE_KEY, JSON.stringify(feedUrls));
		} catch {
			// Ignore storage failures
		}
	}, [feedUrls]);

	useEffect(() => {
		handleLoadScheduler();
	}, [handleLoadScheduler]);

	useEffect(() => {
		handleLoadAgents();
	}, [handleLoadAgents]);

	useEffect(() => {
		handleLoadTrx();
	}, [handleLoadTrx]);

	useEffect(() => {
		feedUrls.forEach((url) => {
			if (!feeds[url]) {
				loadFeed(url);
			}
		});
	}, [feedUrls, feeds, loadFeed]);

	useEffect(() => {
		handleLoadCodexbar();
	}, [handleLoadCodexbar]);

	const topTrxIssues = useMemo(() => {
		return [...trxIssues]
			.sort((a, b) => b.updated_at.localeCompare(a.updated_at))
			.slice(0, 6);
	}, [trxIssues]);

	const scheduleList = scheduler?.schedules ?? [];
	const codexbarEntries = codexbar.payload ?? [];

	return (
		<div className="flex flex-col h-full min-h-0 p-4 md:p-6 gap-4 overflow-y-auto w-full">
			<div className="flex flex-col md:flex-row md:items-end md:justify-between gap-3">
				<div>
					<h1 className="text-2xl md:text-3xl font-semibold tracking-tight">
						{t.title}
					</h1>
					<p className="text-sm text-muted-foreground">{t.subtitle}</p>
				</div>
				<div className="text-xs text-muted-foreground">
					{new Date().toLocaleDateString()}
				</div>
			</div>

			<div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-3">
				<StatCard
					label={t.stats}
					value={`${runningSessions.length} / ${workspaceSessions.length}`}
					subValue="Sessions running"
					Icon={Activity}
					accent="border-cyan-500/30 text-cyan-300"
				/>
				<StatCard
					label="Busy Chats"
					value={busyChatSessions.length}
					subValue={`${opencodeSessions.length} total chats`}
					Icon={Flame}
					accent="border-rose-500/30 text-rose-300"
				/>
				<StatCard
					label="Scheduler"
					value={scheduleStats.enabled}
					subValue={`${scheduleStats.total} total schedules`}
					Icon={CalendarClock}
					accent="border-amber-500/30 text-amber-300"
				/>
				<StatCard
					label="TRX"
					value={trxStats.open}
					subValue={`${trxStats.total} issues tracked`}
					Icon={ListTodo}
					accent="border-emerald-500/30 text-emerald-300"
				/>
			</div>

			<div className="grid grid-cols-1 xl:grid-cols-3 gap-4">
				<div className="xl:col-span-2 flex flex-col gap-4">
					<Card className="border-border/60">
						<CardHeader className="flex flex-row items-center justify-between">
							<div>
								<CardTitle>{t.scheduler}</CardTitle>
								<CardDescription>
									{schedulerError
										? schedulerError
										: `${scheduleStats.enabled} enabled, ${scheduleStats.disabled} disabled`}
								</CardDescription>
							</div>
							<Button
								variant="outline"
								size="sm"
								onClick={handleLoadScheduler}
								disabled={schedulerLoading}
								className="gap-2"
							>
								<RefreshCw className="h-4 w-4" />
								{t.reload}
							</Button>
						</CardHeader>
						<CardContent>
							{scheduleList.length === 0 ? (
								<div className="text-sm text-muted-foreground">
									{t.noTasks}
								</div>
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
											<div className="text-xs text-muted-foreground text-right">
												<div>{humanizeCron(schedule.schedule, locale)}</div>
												<div className="opacity-70">{schedule.schedule}</div>
												{schedule.next_run && (
													<div>Next: {schedule.next_run}</div>
												)}
											</div>
										</div>
									))}
								</div>
							)}
						</CardContent>
					</Card>

					<Card className="border-border/60">
						<CardHeader>
							<CardTitle>{t.workingAgents}</CardTitle>
							<CardDescription>
								{runningSessions.length} running containers, {agents.length} agent profiles
							</CardDescription>
						</CardHeader>
						<CardContent className="space-y-4">
							{runningSessions.length === 0 ? (
								<div className="text-sm text-muted-foreground">
									{t.noAgents}
								</div>
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
									<Badge variant="secondary">
										{busyChatSessions.length}
									</Badge>
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
				</div>

				<div className="flex flex-col gap-4">
					{codexbar.available && (
						<Card className="border-border/60">
							<CardHeader className="flex flex-row items-center justify-between">
								<div>
									<CardTitle>AI Subscriptions</CardTitle>
									<CardDescription>
										CodexBar usage snapshots
									</CardDescription>
								</div>
								<Button
									variant="outline"
									size="sm"
									onClick={handleLoadCodexbar}
									disabled={codexbar.loading}
									className="gap-2"
								>
									<RefreshCw className="h-4 w-4" />
									{t.reload}
								</Button>
							</CardHeader>
							<CardContent className="space-y-3">
								{codexbar.error && (
									<p className="text-xs text-rose-400">{codexbar.error}</p>
								)}
								{codexbarEntries.length === 0 ? (
									<p className="text-sm text-muted-foreground">
										No CodexBar data yet.
									</p>
								) : (
									<div className="space-y-3">
										{codexbarEntries.slice(0, 6).map((entry) => {
											const primary = entry.usage?.primary;
											const secondary = entry.usage?.secondary;
											const credits = entry.credits?.remaining;
											return (
												<div
													key={`${entry.provider}-${entry.account ?? ""}`}
													className="border-b border-border/40 pb-3 last:border-b-0 last:pb-0"
												>
													<div className="flex items-center justify-between gap-2">
														<div className="min-w-0">
															<p className="text-sm font-medium truncate">
																{entry.provider}
															</p>
															<p className="text-xs text-muted-foreground truncate">
																{entry.account ??
																	entry.usage?.accountEmail ??
																	entry.source}
															</p>
														</div>
														{entry.status?.indicator && (
															<Badge variant="secondary">
																{entry.status.indicator}
															</Badge>
														)}
													</div>
													<div className="mt-2 grid grid-cols-2 gap-2 text-xs text-muted-foreground">
														<div>
															Session:{" "}
															{primary?.usedPercent != null
																? `${primary.usedPercent}% used`
																: "n/a"}
															{primary?.resetsAt && (
																<span className="block">
																	Resets {formatDateTime(primary.resetsAt)}
																</span>
															)}
														</div>
														<div>
															Weekly:{" "}
															{secondary?.usedPercent != null
																? `${secondary.usedPercent}% used`
																: "n/a"}
															{secondary?.resetsAt && (
																<span className="block">
																	Resets {formatDateTime(secondary.resetsAt)}
																</span>
															)}
														</div>
													</div>
													{credits != null && (
														<p className="text-xs text-muted-foreground mt-2">
															Credits: {credits}
														</p>
													)}
												</div>
											);
										})}
									</div>
								)}
							</CardContent>
						</Card>
					)}

					<Card className="border-border/60">
						<CardHeader className="flex flex-row items-center justify-between">
							<div>
								<CardTitle>{t.trx}</CardTitle>
								<CardDescription>
									{trxError
										? trxError
										: `${trxStats.open} open, ${trxStats.inProgress} in progress, ${trxStats.blocked} blocked`}
								</CardDescription>
							</div>
							<Button
								variant="outline"
								size="sm"
								onClick={handleLoadTrx}
								disabled={trxLoading}
								className="gap-2"
							>
								<RefreshCw className="h-4 w-4" />
								{t.reload}
							</Button>
						</CardHeader>
						<CardContent className="space-y-3">
							{topTrxIssues.length === 0 ? (
								<div className="text-sm text-muted-foreground">
									{t.noTrx}
								</div>
							) : (
								<div className="space-y-2">
									{topTrxIssues.map((issue) => (
										<div
											key={issue.id}
											className="flex items-start gap-3 border-b border-border/40 pb-2 last:border-b-0 last:pb-0"
										>
											<CheckCircle2 className="h-4 w-4 text-muted-foreground mt-0.5" />
											<div className="min-w-0">
												<p className="text-sm font-medium truncate">
													{issue.title}
												</p>
												<div className="flex items-center gap-2 text-xs text-muted-foreground">
													<Badge variant="outline">{issue.issue_type}</Badge>
													<span className="truncate">{issue.id}</span>
												</div>
											</div>
											<div className="text-xs text-muted-foreground">
												P{issue.priority}
											</div>
										</div>
									))}
								</div>
							)}
						</CardContent>
					</Card>

					<Card className="border-border/60">
						<CardHeader>
							<CardTitle className="flex items-center gap-2">
								<Rss className="h-4 w-4" />
								{t.feeds}
							</CardTitle>
							<CardDescription>RSS / Atom reader</CardDescription>
						</CardHeader>
						<CardContent className="space-y-4">
							<div className="flex gap-2">
								<Input
									placeholder="https://example.com/feed.xml"
									value={feedInput}
									onChange={(event) => setFeedInput(event.target.value)}
									onKeyDown={(event) => {
										if (event.key === "Enter") {
											event.preventDefault();
											handleAddFeed();
										}
									}}
								/>
								<Button onClick={handleAddFeed} className="gap-2">
									<Sparkles className="h-4 w-4" />
									{t.addFeed}
								</Button>
							</div>
							<div className="flex items-center justify-between">
								<p className="text-xs text-muted-foreground">
									{feedUrls.length} feeds tracked
								</p>
								<Button
									variant="ghost"
									size="sm"
									onClick={handleRefreshFeeds}
									className="gap-1"
								>
									<RefreshCw className="h-3.5 w-3.5" />
									{t.reload}
								</Button>
							</div>

							{feedUrls.length === 0 ? (
								<p className="text-sm text-muted-foreground">{t.noFeeds}</p>
							) : (
								<div className="space-y-4">
									{feedUrls.map((url) => {
										const feed = feeds[url];
										return (
											<div
												key={url}
												className="border border-border/50 rounded-lg p-3 space-y-2"
											>
												<div className="flex items-start justify-between gap-2">
													<div className="min-w-0">
														<p className="text-sm font-medium truncate">
															{feed?.title || url}
														</p>
														<p className="text-xs text-muted-foreground truncate">
															{url}
														</p>
													</div>
													<Button
														variant="ghost"
														size="icon"
														onClick={() => handleRemoveFeed(url)}
													>
														<X className="h-4 w-4" />
													</Button>
												</div>
												{feed?.loading ? (
													<p className="text-xs text-muted-foreground">Loading...</p>
												) : feed?.error ? (
													<p className="text-xs text-rose-400">{feed.error}</p>
												) : (
													<ul className="space-y-1">
														{(feed?.items ?? []).slice(0, 4).map((item) => (
															<li
																key={item.id}
																className="text-xs text-muted-foreground"
															>
																{item.link ? (
																	<a
																		href={item.link}
																		target="_blank"
																		rel="noreferrer"
																		className="text-foreground hover:underline"
																	>
																		{item.title}
																	</a>
																) : (
																	<span className="text-foreground">
																		{item.title}
																	</span>
																)}
																{item.date && (
																	<span className="ml-2">{formatDateTime(item.date)}</span>
																)}
															</li>
														))}
													</ul>
												)}
											</div>
										);
									})}
								</div>
							)}
						</CardContent>
					</Card>
				</div>
			</div>
		</div>
	);
}

export default DashboardApp;
