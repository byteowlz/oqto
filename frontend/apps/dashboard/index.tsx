"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { MarkdownRenderer } from "@/components/ui/markdown-renderer";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { useApp } from "@/hooks/use-app";
import { useIsMobile } from "@/hooks/use-mobile";
import {
	type CodexBarUsagePayload,
	type SchedulerOverview,
	controlPlaneApiUrl,
	fetchFeed,
	fileserverWorkspaceBaseUrl,
	getAuthHeaders,
	getCodexBarUsage,
	getSchedulerOverview,
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
	GripVertical,
	ListTodo,
	PanelLeftClose,
	PanelRightClose,
	RefreshCw,
	Rss,
	Sparkles,
	Trash2,
	X,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";

const DASHBOARD_CONFIG_PATH = ".octo/dashboard.json";
const DASHBOARD_REGISTRY_PATH = ".octo/dashboard.registry.json";
const LEGACY_FEED_STORAGE_KEY = "octo:dashboardFeeds";
const GRID_ROW_HEIGHT_REM = 14;

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

type DashboardCardSpan = 3 | 6 | 9 | 12;

type DashboardLayoutCard = {
	visible: boolean;
	span: DashboardCardSpan;
};

type DashboardLayoutConfig = {
	version: 1;
	order: string[];
	cards: Record<string, DashboardLayoutCard>;
	feeds?: string[];
};

type DashboardRegistryCard = {
	id: string;
	title: string;
	description?: string;
	kind: "markdown" | "query";
	config?: {
		content?: string;
		url?: string;
		method?: string;
		headers?: Record<string, string>;
	};
};

type DashboardRegistryConfig = {
	version: 1;
	cards: DashboardRegistryCard[];
};

type BuiltinCardDefinition = {
	id: string;
	title: string;
	description?: string;
	defaultSpan: DashboardCardSpan;
};

const CARD_SPAN_OPTIONS: { value: DashboardCardSpan; label: string }[] = [
	{ value: 3, label: "1x" },
	{ value: 6, label: "2x" },
	{ value: 9, label: "3x" },
	{ value: 12, label: "Full" },
];

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

function parseFeedXml(xml: string): FeedState {
	const parser = new DOMParser();
	const doc = parser.parseFromString(xml, "application/xml");

	const atomFeed = doc.querySelector("feed");
	const rssFeed = doc.querySelector("rss, RDF");

	if (atomFeed) {
		const title =
			atomFeed.querySelector("title")?.textContent?.trim() ?? "Atom Feed";
		const entries = Array.from(atomFeed.querySelectorAll("entry")).map(
			(entry) => {
				const id = entry.querySelector("id")?.textContent?.trim() ?? createId();
				const link =
					entry.querySelector("link[rel='alternate']")?.getAttribute("href") ??
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

async function readWorkspaceFile(
	workspacePath: string,
	path: string,
): Promise<string | null> {
	const origin =
		typeof window !== "undefined" ? window.location.origin : "http://localhost";
	const url = new URL(`${fileserverWorkspaceBaseUrl()}/file`, origin);
	url.searchParams.set("workspace_path", workspacePath);
	url.searchParams.set("path", path);
	const res = await fetch(url.toString(), {
		headers: {
			...getAuthHeaders(),
		},
		credentials: "include",
	});
	if (res.status === 404 || res.status === 502 || res.status === 503)
		return null;
	if (!res.ok) {
		const text = await res.text();
		throw new Error(text || `Failed to read ${path}`);
	}
	return res.text();
}

async function writeWorkspaceFile(
	workspacePath: string,
	path: string,
	content: string,
): Promise<void> {
	const origin =
		typeof window !== "undefined" ? window.location.origin : "http://localhost";
	const url = new URL(`${fileserverWorkspaceBaseUrl()}/file`, origin);
	url.searchParams.set("workspace_path", workspacePath);
	url.searchParams.set("path", path);
	url.searchParams.set("mkdir", "true");
	const res = await fetch(url.toString(), {
		method: "PUT",
		body: content,
		headers: {
			"Content-Type": "application/json",
			...getAuthHeaders(),
		},
		credentials: "include",
	});
	if (!res.ok) {
		const text = await res.text();
		throw new Error(text || `Failed to write ${path}`);
	}
}

function spanToClass(span: DashboardCardSpan): string {
	switch (span) {
		case 3:
			return "lg:col-span-3";
		case 6:
			return "lg:col-span-6";
		case 9:
			return "lg:col-span-9";
		case 12:
			return "lg:col-span-12";
		default:
			return "lg:col-span-6";
	}
}

function clampSpan(value: number): DashboardCardSpan {
	if (value <= 3) return 3;
	if (value <= 6) return 6;
	if (value <= 9) return 9;
	return 12;
}

function buildDefaultLayout(
	cardIds: string[],
	defaultSpans: Record<string, DashboardCardSpan>,
	feeds: string[],
): DashboardLayoutConfig {
	const cards: Record<string, DashboardLayoutCard> = {};
	for (const id of cardIds) {
		cards[id] = {
			visible: true,
			span: defaultSpans[id] ?? 6,
		};
	}
	return {
		version: 1,
		order: [...cardIds],
		cards,
		feeds,
	};
}

function normalizeLayout(
	layout: DashboardLayoutConfig,
	cardIds: string[],
	defaultSpans: Record<string, DashboardCardSpan>,
): DashboardLayoutConfig {
	const nextOrder = layout.order.filter((id) => cardIds.includes(id));
	const known = new Set(nextOrder);
	for (const id of cardIds) {
		if (!known.has(id)) {
			nextOrder.push(id);
			known.add(id);
		}
	}

	const nextCards: Record<string, DashboardLayoutCard> = { ...layout.cards };
	for (const id of cardIds) {
		if (!nextCards[id]) {
			nextCards[id] = { visible: true, span: defaultSpans[id] ?? 6 };
		} else {
			nextCards[id] = {
				...nextCards[id],
				span: clampSpan(nextCards[id].span),
			};
		}
	}

	return {
		...layout,
		order: nextOrder,
		cards: nextCards,
	};
}

const StatCard = memo(function StatCard({
	label,
	value,
	subValue,
	Icon,
	accent,
	className,
}: {
	label: string;
	value: string | number;
	subValue?: string;
	Icon: React.ElementType;
	accent?: string;
	className?: string;
}) {
	return (
		<Card
			className={cn(
				"border border-border bg-muted/30 shadow-none h-full",
				className,
			)}
		>
			<CardContent className="p-4 flex items-start justify-between h-full">
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
						"p-2 rounded-lg border self-start",
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

function CollapsedSidebarButton({
	active,
	label,
	icon: Icon,
	onClick,
}: {
	active: boolean;
	label: string;
	icon: React.ElementType;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={cn(
				"w-8 h-8 flex items-center justify-center relative transition-colors rounded",
				active
					? "bg-primary/15 text-foreground border border-primary"
					: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
			)}
			aria-label={label}
			title={label}
		>
			<Icon className="h-4 w-4" />
		</button>
	);
}

function MobileTabButton({
	active,
	label,
	icon: Icon,
	onClick,
}: {
	active: boolean;
	label: string;
	icon: React.ElementType;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={cn(
				"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
				active
					? "bg-primary/15 text-foreground border border-primary"
					: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
			)}
			title={label}
		>
			<Icon className="h-4 w-4" />
		</button>
	);
}

function QueryCard({
	title,
	description,
	url,
	method,
	headers,
}: {
	title: string;
	description?: string;
	url?: string;
	method?: string;
	headers?: Record<string, string>;
}) {
	const [data, setData] = useState<unknown>(null);
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);

	const handleLoad = useCallback(async () => {
		if (!url) return;
		setLoading(true);
		setError(null);
		try {
			const res = await fetch(url, {
				method: method ?? "GET",
				headers,
				credentials: "include",
			});
			const contentType = res.headers.get("content-type") ?? "";
			if (!res.ok) {
				const text = await res.text();
				throw new Error(text || "Query failed");
			}
			if (contentType.includes("application/json")) {
				setData(await res.json());
			} else {
				setData(await res.text());
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : "Query failed");
		} finally {
			setLoading(false);
		}
	}, [headers, method, url]);

	useEffect(() => {
		if (url) {
			handleLoad();
		}
	}, [handleLoad, url]);

	return (
		<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
			<CardHeader className="flex flex-row items-center justify-between">
				<div>
					<CardTitle>{title}</CardTitle>
					{description && <CardDescription>{description}</CardDescription>}
				</div>
				<Button
					variant="outline"
					size="sm"
					onClick={handleLoad}
					disabled={loading || !url}
					className="gap-2"
				>
					<RefreshCw className="h-4 w-4" />
					Refresh
				</Button>
			</CardHeader>
			<CardContent className="flex-1 overflow-auto">
				{!url ? (
					<p className="text-sm text-muted-foreground">No URL configured.</p>
				) : loading ? (
					<p className="text-sm text-muted-foreground">Loading...</p>
				) : error ? (
					<p className="text-sm text-rose-400">{error}</p>
				) : (
					<pre className="text-xs whitespace-pre-wrap text-muted-foreground">
						{typeof data === "string" ? data : JSON.stringify(data, null, 2)}
					</pre>
				)}
			</CardContent>
		</Card>
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
		mainChatWorkspacePath,
	} = useApp();
	const [scheduler, setScheduler] = useState<SchedulerOverview | null>(null);
	const [schedulerError, setSchedulerError] = useState<string | null>(null);
	const [schedulerLoading, setSchedulerLoading] = useState(false);
	const [agents, setAgents] = useState<OpenCodeAgent[]>([]);
	const [trxIssues, setTrxIssues] = useState<TrxIssue[]>([]);
	const [trxError, setTrxError] = useState<string | null>(null);
	const [trxLoading, setTrxLoading] = useState(false);
	const [feeds, setFeeds] = useState<Record<string, FeedState>>({});
	const mountedRef = useRef(true);
	const [codexbar, setCodexbar] = useState<CodexBarState>({
		available: false,
		loading: false,
		payload: [],
	});
	const [layoutConfig, setLayoutConfig] =
		useState<DashboardLayoutConfig | null>(null);
	const layoutRef = useRef<DashboardLayoutConfig | null>(null);
	const [registryCards, setRegistryCards] = useState<DashboardRegistryCard[]>(
		[],
	);
	const registryRef = useRef<DashboardRegistryCard[]>([]);
	const [layoutError, setLayoutError] = useState<string | null>(null);
	const [layoutLoading, setLayoutLoading] = useState(false);
	const [draggedCardId, setDraggedCardId] = useState<string | null>(null);
	const [feedInput, setFeedInput] = useState("");
	const [customTitle, setCustomTitle] = useState("");
	const [customDescription, setCustomDescription] = useState("");
	const [customType, setCustomType] = useState<"markdown" | "query">(
		"markdown",
	);
	const [customContent, setCustomContent] = useState("");
	const [customUrl, setCustomUrl] = useState("");
	const [customMethod, setCustomMethod] = useState("GET");
	const [rightSidebarCollapsed, setRightSidebarCollapsed] = useState(false);
	const [sidebarSection, setSidebarSection] = useState<"cards" | "custom">(
		"cards",
	);
	const [layoutEditMode, setLayoutEditMode] = useState(false);
	const isMobileLayout = useIsMobile();
	const [mobileView, setMobileView] = useState<
		"dashboard" | "cards" | "custom"
	>("dashboard");

	const workspacePath =
		selectedWorkspaceSession?.workspace_path ?? opencodeDirectory ?? ".";
	const configWorkspacePath = mainChatWorkspacePath ?? workspacePath;

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
				layout: "Layout",
				registry: "Card Registry",
				customCards: "Custom Cards",
				notice: "Config in .octo/ for the Main Chat workspace.",
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
				layout: "Layout",
				registry: "Card Registry",
				customCards: "Custom Cards",
				notice: "Config lives in .octo/ for the Main Chat workspace.",
			},
		}),
		[],
	);
	const t = copy[locale];

	const builtinCards: BuiltinCardDefinition[] = useMemo(
		() => [
			{
				id: "stat-sessions",
				title: "Sessions",
				defaultSpan: 3,
			},
			{
				id: "stat-busy",
				title: "Busy Chats",
				defaultSpan: 3,
			},
			{
				id: "stat-scheduler",
				title: "Scheduler",
				defaultSpan: 3,
			},
			{
				id: "stat-trx",
				title: "TRX",
				defaultSpan: 3,
			},
			{
				id: "scheduler",
				title: t.scheduler,
				defaultSpan: 6,
			},
			{
				id: "agents",
				title: t.workingAgents,
				defaultSpan: 6,
			},
			{
				id: "trx",
				title: t.trx,
				defaultSpan: 6,
			},
			{
				id: "codexbar",
				title: "AI Subscriptions",
				defaultSpan: 6,
			},
			{
				id: "feeds",
				title: t.feeds,
				defaultSpan: 6,
			},
		],
		[t],
	);

	const builtinDefaultSpans = useMemo(() => {
		return builtinCards.reduce<Record<string, DashboardCardSpan>>(
			(acc, card) => {
				acc[card.id] = card.defaultSpan;
				return acc;
			},
			{},
		);
	}, [builtinCards]);

	const customDefaultSpans = useMemo(() => {
		return registryCards.reduce<Record<string, DashboardCardSpan>>(
			(acc, card) => {
				acc[card.id] = 6;
				return acc;
			},
			{},
		);
	}, [registryCards]);

	const cardIdList = useMemo(() => {
		return [
			...builtinCards.map((card) => card.id),
			...registryCards.map((card) => card.id),
		];
	}, [builtinCards, registryCards]);

	const defaultSpans = useMemo(() => {
		return { ...builtinDefaultSpans, ...customDefaultSpans };
	}, [builtinDefaultSpans, customDefaultSpans]);

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

	const handleRefreshFeeds = useCallback(
		(feedUrls: string[]) => {
			for (const url of feedUrls) {
				loadFeed(url);
			}
		},
		[loadFeed],
	);

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

	const persistLayout = useCallback(
		async (next: DashboardLayoutConfig) => {
			if (!configWorkspacePath) return;
			try {
				await writeWorkspaceFile(
					configWorkspacePath,
					DASHBOARD_CONFIG_PATH,
					JSON.stringify(next, null, 2),
				);
			} catch (err) {
				console.error("Failed to save dashboard layout:", err);
			}
		},
		[configWorkspacePath],
	);

	const updateLayout = useCallback(
		(updater: (layout: DashboardLayoutConfig) => DashboardLayoutConfig) => {
			if (!layoutRef.current) return;
			const next = updater(layoutRef.current);
			layoutRef.current = next;
			setLayoutConfig(next);
			void persistLayout(next);
		},
		[persistLayout],
	);

	const persistRegistry = useCallback(
		async (next: DashboardRegistryCard[]) => {
			if (!configWorkspacePath) return;
			const payload: DashboardRegistryConfig = {
				version: 1,
				cards: next,
			};
			try {
				await writeWorkspaceFile(
					configWorkspacePath,
					DASHBOARD_REGISTRY_PATH,
					JSON.stringify(payload, null, 2),
				);
			} catch (err) {
				console.error("Failed to save dashboard registry:", err);
			}
		},
		[configWorkspacePath],
	);

	useEffect(() => {
		mountedRef.current = true;
		return () => {
			mountedRef.current = false;
		};
	}, []);

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
		handleLoadCodexbar();
	}, [handleLoadCodexbar]);

	useEffect(() => {
		if (!configWorkspacePath) return;
		let active = true;
		setLayoutLoading(true);
		setLayoutError(null);
		(async () => {
			try {
				const [layoutRaw, registryRaw] = await Promise.all([
					readWorkspaceFile(configWorkspacePath, DASHBOARD_CONFIG_PATH),
					readWorkspaceFile(configWorkspacePath, DASHBOARD_REGISTRY_PATH),
				]);

				let registry: DashboardRegistryCard[] = [];
				if (registryRaw) {
					try {
						const parsed = JSON.parse(registryRaw) as DashboardRegistryConfig;
						registry = parsed.cards ?? [];
					} catch (err) {
						console.warn("Failed to parse dashboard registry:", err);
					}
				}

				const registryIds = new Set(registry.map((card) => card.id));
				const sanitizedRegistry = registry.filter(
					(card) => card.id && card.title,
				);
				const customCards = sanitizedRegistry.filter((card) =>
					registryIds.has(card.id),
				);
				const mergedRegistry = customCards;
				const allCardIds = [
					...builtinCards.map((card) => card.id),
					...mergedRegistry.map((card) => card.id),
				];

				let feedsFromStorage: string[] = [];
				if (typeof window !== "undefined") {
					try {
						const stored = localStorage.getItem(LEGACY_FEED_STORAGE_KEY);
						feedsFromStorage = stored ? (JSON.parse(stored) as string[]) : [];
					} catch {
						feedsFromStorage = [];
					}
				}

				let layout: DashboardLayoutConfig | null = null;
				if (layoutRaw) {
					try {
						layout = JSON.parse(layoutRaw) as DashboardLayoutConfig;
					} catch (err) {
						console.warn("Failed to parse dashboard layout:", err);
					}
				}

				const spanDefaults = {
					...builtinDefaultSpans,
					...mergedRegistry.reduce<Record<string, DashboardCardSpan>>(
						(acc, card) => {
							acc[card.id] = 6;
							return acc;
						},
						{},
					),
				};

				let normalizedLayout = layout;
				if (!normalizedLayout || normalizedLayout.version !== 1) {
					normalizedLayout = buildDefaultLayout(
						allCardIds,
						spanDefaults,
						feedsFromStorage,
					);
				} else {
					normalizedLayout = normalizeLayout(
						{
							...normalizedLayout,
							feeds: normalizedLayout.feeds ?? feedsFromStorage,
						},
						allCardIds,
						spanDefaults,
					);
				}

				if (!active) return;

				registryRef.current = mergedRegistry;
				setRegistryCards(mergedRegistry);
				layoutRef.current = normalizedLayout;
				setLayoutConfig(normalizedLayout);

				if (!layoutRaw || !registryRaw) {
					void persistLayout(normalizedLayout);
					if (!registryRaw) {
						void persistRegistry(mergedRegistry);
					}
				}
			} catch (err) {
				if (!active) return;
				console.error("Failed to load dashboard config:", err);
				setLayoutError(
					err instanceof Error ? err.message : "Failed to load layout",
				);
			} finally {
				if (active) setLayoutLoading(false);
			}
		})();

		return () => {
			active = false;
		};
	}, [
		builtinCards,
		builtinDefaultSpans,
		configWorkspacePath,
		persistLayout,
		persistRegistry,
	]);

	useEffect(() => {
		if (!layoutConfig) return;
		for (const url of layoutConfig.feeds ?? []) {
			if (!feeds[url]) {
				loadFeed(url);
			}
		}
	}, [layoutConfig, feeds, loadFeed]);

	const handleAddFeed = useCallback(() => {
		const trimmed = feedInput.trim();
		if (!trimmed) return;
		if (!layoutRef.current) return;
		if (layoutRef.current.feeds?.includes(trimmed)) {
			setFeedInput("");
			return;
		}
		const next = [trimmed, ...(layoutRef.current.feeds ?? [])].slice(0, 8);
		updateLayout((prev) => ({ ...prev, feeds: next }));
		setFeedInput("");
	}, [feedInput, updateLayout]);

	const handleRemoveFeed = useCallback(
		(url: string) => {
			if (!layoutRef.current) return;
			const next = (layoutRef.current.feeds ?? []).filter(
				(entry) => entry !== url,
			);
			updateLayout((prev) => ({ ...prev, feeds: next }));
			setFeeds((prev) => {
				const updated = { ...prev };
				delete updated[url];
				return updated;
			});
		},
		[updateLayout],
	);

	const topTrxIssues = useMemo(() => {
		return [...trxIssues]
			.sort((a, b) => b.updated_at.localeCompare(a.updated_at))
			.slice(0, 6);
	}, [trxIssues]);

	const scheduleList = scheduler?.schedules ?? [];
	const codexbarEntries = codexbar.payload ?? [];
	const feedUrls = layoutConfig?.feeds ?? [];

	const cardMap = useMemo(() => {
		const map = new Map<
			string,
			BuiltinCardDefinition | DashboardRegistryCard
		>();
		for (const card of builtinCards) {
			map.set(card.id, card);
		}
		for (const card of registryCards) {
			map.set(card.id, card);
		}
		return map;
	}, [builtinCards, registryCards]);

	const orderedCards = useMemo(() => {
		if (!layoutConfig) return [];
		return layoutConfig.order
			.map((id) => cardMap.get(id))
			.filter(Boolean) as Array<BuiltinCardDefinition | DashboardRegistryCard>;
	}, [cardMap, layoutConfig]);

	const visibleCards = useMemo(() => {
		if (!layoutConfig) return [];
		return orderedCards.filter(
			(card) => layoutConfig.cards[card.id]?.visible !== false,
		);
	}, [layoutConfig, orderedCards]);

	const handleToggleCard = useCallback(
		(id: string) => {
			updateLayout((prev) => ({
				...prev,
				cards: {
					...prev.cards,
					[id]: {
						...prev.cards[id],
						visible: !prev.cards[id]?.visible,
						span: prev.cards[id]?.span ?? 6,
					},
				},
			}));
		},
		[updateLayout],
	);

	const handleSpanChange = useCallback(
		(id: string, span: DashboardCardSpan) => {
			updateLayout((prev) => ({
				...prev,
				cards: {
					...prev.cards,
					[id]: {
						...prev.cards[id],
						span,
					},
				},
			}));
		},
		[updateLayout],
	);

	const handleDragStart = useCallback((id: string) => {
		setDraggedCardId(id);
	}, []);

	const handleDrop = useCallback(
		(targetId: string) => {
			if (!draggedCardId || !layoutRef.current) return;
			if (draggedCardId === targetId) return;
			updateLayout((prev) => {
				const order = [...prev.order];
				const fromIndex = order.indexOf(draggedCardId);
				const toIndex = order.indexOf(targetId);
				if (fromIndex === -1 || toIndex === -1) return prev;
				order.splice(fromIndex, 1);
				order.splice(toIndex, 0, draggedCardId);
				return { ...prev, order };
			});
			setDraggedCardId(null);
		},
		[draggedCardId, updateLayout],
	);

	const handleDragEnd = useCallback(() => {
		setDraggedCardId(null);
	}, []);

	const handleAddCustomCard = useCallback(() => {
		const title = customTitle.trim();
		if (!title) return;
		const id = `custom-${createId()}`;
		const newCard: DashboardRegistryCard = {
			id,
			title,
			description: customDescription.trim() || undefined,
			kind: customType,
			config:
				customType === "markdown"
					? { content: customContent }
					: { url: customUrl, method: customMethod },
		};

		const nextRegistry = [...registryRef.current, newCard];
		registryRef.current = nextRegistry;
		setRegistryCards(nextRegistry);
		void persistRegistry(nextRegistry);

		updateLayout((prev) => {
			const nextCards = {
				...prev.cards,
				[newCard.id]: { visible: true, span: 6 },
			};
			return {
				...prev,
				order: [...prev.order, newCard.id],
				cards: nextCards,
			};
		});

		setCustomTitle("");
		setCustomDescription("");
		setCustomContent("");
		setCustomUrl("");
	}, [
		customContent,
		customDescription,
		customMethod,
		customTitle,
		customType,
		customUrl,
		persistRegistry,
		updateLayout,
	]);

	const handleRemoveCustomCard = useCallback(
		(id: string) => {
			const nextRegistry = registryRef.current.filter((card) => card.id !== id);
			registryRef.current = nextRegistry;
			setRegistryCards(nextRegistry);
			void persistRegistry(nextRegistry);
			updateLayout((prev) => {
				const nextCards = { ...prev.cards };
				delete nextCards[id];
				return {
					...prev,
					order: prev.order.filter((entry) => entry !== id),
					cards: nextCards,
				};
			});
		},
		[persistRegistry, updateLayout],
	);

	const renderBuiltinCard = (id: string) => {
		switch (id) {
			case "stat-sessions":
				return (
					<StatCard
						label={t.stats}
						value={`${runningSessions.length} / ${workspaceSessions.length}`}
						subValue="Sessions running"
						Icon={Activity}
						accent="border-cyan-500/30 text-cyan-300"
					/>
				);
			case "stat-busy":
				return (
					<StatCard
						label="Busy Chats"
						value={busyChatSessions.length}
						subValue={`${opencodeSessions.length} total chats`}
						Icon={Flame}
						accent="border-rose-500/30 text-rose-300"
					/>
				);
			case "stat-scheduler":
				return (
					<StatCard
						label="Scheduler"
						value={scheduleStats.enabled}
						subValue={`${scheduleStats.total} total schedules`}
						Icon={CalendarClock}
						accent="border-amber-500/30 text-amber-300"
					/>
				);
			case "stat-trx":
				return (
					<StatCard
						label="TRX"
						value={trxStats.open}
						subValue={`${trxStats.total} issues tracked`}
						Icon={ListTodo}
						accent="border-emerald-500/30 text-emerald-300"
					/>
				);
			case "scheduler":
				return (
					<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
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
						<CardContent className="flex-1 min-h-0 overflow-auto">
							{scheduleList.length === 0 ? (
								<div className="text-sm text-muted-foreground">{t.noTasks}</div>
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
				);
			case "agents":
				return (
					<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
						<CardHeader>
							<CardTitle>{t.workingAgents}</CardTitle>
							<CardDescription>
								{runningSessions.length} running containers, {agents.length}{" "}
								agent profiles
							</CardDescription>
						</CardHeader>
						<CardContent className="flex-1 min-h-0 overflow-auto space-y-4">
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
									<Badge variant="secondary">{busyChatSessions.length}</Badge>
								</div>
								{busyChatSessions.length === 0 ? (
									<p className="text-sm text-muted-foreground">
										No chats busy.
									</p>
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
			case "trx":
				return (
					<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
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
						<CardContent className="flex-1 min-h-0 overflow-auto space-y-3">
							{topTrxIssues.length === 0 ? (
								<div className="text-sm text-muted-foreground">{t.noTrx}</div>
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
				);
			case "codexbar":
				return (
					<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
						<CardHeader className="flex flex-row items-center justify-between">
							<div>
								<CardTitle>AI Subscriptions</CardTitle>
								<CardDescription>CodexBar usage snapshots</CardDescription>
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
						<CardContent className="flex-1 min-h-0 overflow-auto space-y-3">
							{!codexbar.available ? (
								<p className="text-sm text-muted-foreground">
									CodexBar is not available on this host.
								</p>
							) : (
								<>
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
								</>
							)}
						</CardContent>
					</Card>
				);
			case "feeds":
				return (
					<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
						<CardHeader>
							<CardTitle className="flex items-center gap-2">
								<Rss className="h-4 w-4" />
								{t.feeds}
							</CardTitle>
							<CardDescription>RSS / Atom reader</CardDescription>
						</CardHeader>
						<CardContent className="flex-1 min-h-0 overflow-auto space-y-4">
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
									onClick={() => handleRefreshFeeds(feedUrls)}
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
												className="border border-border rounded-lg p-3 space-y-2"
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
													<p className="text-xs text-muted-foreground">
														Loading...
													</p>
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
																	<span className="ml-2">
																		{formatDateTime(item.date)}
																	</span>
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
				);
			default:
				return null;
		}
	};

	const renderCustomCard = (card: DashboardRegistryCard) => {
		if (card.kind === "markdown") {
			return (
				<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
					<CardHeader>
						<CardTitle>{card.title}</CardTitle>
						{card.description && (
							<CardDescription>{card.description}</CardDescription>
						)}
					</CardHeader>
					<CardContent className="flex-1 min-h-0 overflow-auto">
						<MarkdownRenderer content={card.config?.content || ""} />
					</CardContent>
				</Card>
			);
		}

		return (
			<QueryCard
				title={card.title}
				description={card.description}
				url={card.config?.url}
				method={card.config?.method}
				headers={card.config?.headers}
			/>
		);
	};

	const renderCardsManager = () => {
		if (!layoutConfig) {
			return (
				<p className="text-xs text-muted-foreground">Loading dashboard...</p>
			);
		}

		if (orderedCards.length === 0) {
			return (
				<p className="text-xs text-muted-foreground">No cards available.</p>
			);
		}

		return (
			<div className="space-y-2">
				{orderedCards.map((card) => {
					const config = layoutConfig.cards[card.id];
					const visible = config?.visible !== false;
					const isCustom = !("defaultSpan" in card);
					return (
						<div
							key={card.id}
							className="flex flex-col gap-2 rounded-md border border-border bg-muted/30 px-3 py-2"
						>
							<div className="flex items-center justify-between gap-2">
								<div className="flex items-center gap-2 min-w-0">
									<span className="text-sm font-medium truncate">
										{card.title}
									</span>
								</div>
								<div className="flex items-center gap-2">
									<Button
										variant="ghost"
										size="icon"
										onClick={() => handleToggleCard(card.id)}
									>
										{visible ? "Hide" : "Show"}
									</Button>
									{isCustom && (
										<Button
											variant="ghost"
											size="icon"
											onClick={() => handleRemoveCustomCard(card.id)}
										>
											<Trash2 className="h-4 w-4" />
										</Button>
									)}
								</div>
							</div>
						</div>
					);
				})}
			</div>
		);
	};

	const renderCustomManager = () => (
		<div className="space-y-3">
			<Input
				placeholder="Card title"
				value={customTitle}
				onChange={(event) => setCustomTitle(event.target.value)}
			/>
			<Input
				placeholder="Description (optional)"
				value={customDescription}
				onChange={(event) => setCustomDescription(event.target.value)}
			/>
			<Select
				value={customType}
				onValueChange={(value) => setCustomType(value as "markdown" | "query")}
			>
				<SelectTrigger size="sm">
					<SelectValue placeholder="Card type" />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="markdown">Markdown</SelectItem>
					<SelectItem value="query">Query</SelectItem>
				</SelectContent>
			</Select>
			{customType === "markdown" ? (
				<Textarea
					placeholder="Markdown content"
					value={customContent}
					onChange={(event) => setCustomContent(event.target.value)}
					rows={4}
				/>
			) : (
				<div className="space-y-2">
					<Input
						placeholder="https://api.example.com/status"
						value={customUrl}
						onChange={(event) => setCustomUrl(event.target.value)}
					/>
					<Input
						placeholder="GET"
						value={customMethod}
						onChange={(event) => setCustomMethod(event.target.value)}
					/>
				</div>
			)}
			<Button onClick={handleAddCustomCard} className="w-full">
				Add card
			</Button>
		</div>
	);

	const renderDashboardGrid = () => {
		if (layoutLoading || !layoutConfig) {
			return (
				<div className="text-sm text-muted-foreground">
					Loading dashboard...
				</div>
			);
		}

		return (
			<div
				className="grid grid-cols-12 gap-4 auto-rows-fr overflow-y-auto pr-1"
				style={{ gridAutoRows: `${GRID_ROW_HEIGHT_REM}rem` }}
			>
				{visibleCards.map((card) => {
					const config = layoutConfig.cards[card.id];
					const span = config?.span ?? 6;
					return (
						<div
							key={card.id}
							draggable={layoutEditMode}
							onDragStart={() => handleDragStart(card.id)}
							onDragOver={(event) => {
								if (!layoutEditMode) return;
								event.preventDefault();
							}}
							onDrop={() => {
								if (!layoutEditMode) return;
								handleDrop(card.id);
							}}
							onDragEnd={handleDragEnd}
							className={cn("col-span-12", spanToClass(span))}
						>
							<div
								className={cn(
									"relative h-full",
									layoutEditMode && "ring-1 ring-primary/40 rounded-lg",
								)}
							>
								{layoutEditMode && (
									<div className="absolute top-2 right-2 z-10 flex items-center gap-1">
										<Button variant="secondary" size="icon" className="h-7 w-7">
											<GripVertical className="h-4 w-4" />
										</Button>
										{CARD_SPAN_OPTIONS.map((option) => (
											<Button
												key={option.value}
												variant={span === option.value ? "default" : "ghost"}
												size="icon"
												className="h-7 w-7 text-[10px]"
												onClick={() => handleSpanChange(card.id, option.value)}
											>
												{option.label}
											</Button>
										))}
									</div>
								)}
								{"defaultSpan" in card
									? renderBuiltinCard(card.id)
									: renderCustomCard(card)}
							</div>
						</div>
					);
				})}
			</div>
		);
	};

	const activeSidebarSection =
		isMobileLayout && mobileView !== "dashboard" ? mobileView : sidebarSection;

	return (
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4 overflow-hidden w-full">
			{/* Mobile layout */}
			<div className="flex-1 min-h-0 flex flex-col lg:hidden">
				<div className="sticky top-0 z-10 bg-card border border-border rounded-t-xl overflow-hidden">
					<div className="flex gap-0.5 p-1 sm:p-2">
						<MobileTabButton
							active={mobileView === "dashboard"}
							icon={Activity}
							label={t.title}
							onClick={() => setMobileView("dashboard")}
						/>
						<MobileTabButton
							active={mobileView === "cards"}
							icon={ListTodo}
							label="Cards"
							onClick={() => {
								setMobileView("cards");
								setSidebarSection("cards");
							}}
						/>
						<MobileTabButton
							active={mobileView === "custom"}
							icon={Sparkles}
							label={t.customCards}
							onClick={() => {
								setMobileView("custom");
								setSidebarSection("custom");
							}}
						/>
					</div>
				</div>
				<div className="flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-3 sm:p-4 overflow-hidden flex flex-col gap-4">
					<div className="relative flex items-start justify-center gap-3">
						<div className="text-center">
							<h1 className="text-xl font-semibold tracking-tight">
								{t.title}
							</h1>
							<p className="text-sm text-muted-foreground">{t.subtitle}</p>
						</div>
						<div className="absolute right-0 top-0 flex items-center gap-2 text-xs text-muted-foreground">
							{new Date().toLocaleDateString()}
							<Button
								variant={layoutEditMode ? "secondary" : "ghost"}
								size="icon"
								className="size-7"
								onClick={() => setLayoutEditMode((prev) => !prev)}
							>
								<GripVertical className="size-4" />
							</Button>
						</div>
					</div>

					{layoutError && (
						<div className="text-sm text-rose-400">{layoutError}</div>
					)}

					<div className="flex-1 min-h-0 overflow-hidden">
						{mobileView === "dashboard" ? (
							renderDashboardGrid()
						) : (
							<div className="h-full overflow-y-auto">
								{activeSidebarSection === "cards"
									? renderCardsManager()
									: renderCustomManager()}
							</div>
						)}
					</div>
				</div>
			</div>

			{/* Desktop layout */}
			<div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
				<div className="flex-[3] min-w-0 bg-card border border-border p-4 xl:p-5 flex flex-col min-h-0 h-full">
					<div className="flex items-start justify-between gap-3">
						<div>
							<h1 className="text-2xl md:text-3xl font-semibold tracking-tight">
								{t.title}
							</h1>
							<p className="text-sm text-muted-foreground">{t.subtitle}</p>
						</div>
						<div className="flex items-center gap-2 text-xs text-muted-foreground">
							{new Date().toLocaleDateString()}
							<Button
								variant={layoutEditMode ? "secondary" : "ghost"}
								size="icon"
								className="size-7"
								onClick={() => setLayoutEditMode((prev) => !prev)}
							>
								<GripVertical className="size-4" />
							</Button>
							<button
								type="button"
								onClick={() => setRightSidebarCollapsed((prev) => !prev)}
								className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
								title={
									rightSidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"
								}
							>
								{rightSidebarCollapsed ? (
									<PanelLeftClose className="size-4" />
								) : (
									<PanelRightClose className="size-4" />
								)}
							</button>
						</div>
					</div>

					{layoutError && (
						<div className="text-sm text-rose-400 mt-2">{layoutError}</div>
					)}

					<div className="flex-1 min-h-0 overflow-hidden mt-4">
						{renderDashboardGrid()}
					</div>
				</div>

				<div
					className={cn(
						"bg-card border border-border flex flex-col min-h-0 h-full transition-all duration-200",
						rightSidebarCollapsed
							? "w-12 items-center"
							: "flex-[2] min-w-[320px] max-w-[420px]",
					)}
				>
					{rightSidebarCollapsed ? (
						<div className="flex flex-col gap-1 p-2 h-full overflow-y-auto">
							<CollapsedSidebarButton
								active={sidebarSection === "cards"}
								label="Cards"
								icon={ListTodo}
								onClick={() => {
									setSidebarSection("cards");
									setRightSidebarCollapsed(false);
								}}
							/>
							<CollapsedSidebarButton
								active={sidebarSection === "custom"}
								label="Custom cards"
								icon={Sparkles}
								onClick={() => {
									setSidebarSection("custom");
									setRightSidebarCollapsed(false);
								}}
							/>
						</div>
					) : (
						<div className="flex flex-col h-full min-h-0">
							<div className="px-4 py-3 border-b border-border">
								<div>
									<p className="text-sm font-semibold">{t.layout}</p>
									<p className="text-xs text-muted-foreground">{t.notice}</p>
								</div>
								<div className="mt-3 flex gap-1">
									<button
										type="button"
										onClick={() => setSidebarSection("cards")}
										className={cn(
											"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
											sidebarSection === "cards"
												? "bg-primary/15 text-foreground border border-primary"
												: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
										)}
										title="Cards"
									>
										<ListTodo className="h-4 w-4" />
									</button>
									<button
										type="button"
										onClick={() => setSidebarSection("custom")}
										className={cn(
											"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
											sidebarSection === "custom"
												? "bg-primary/15 text-foreground border border-primary"
												: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
										)}
										title={t.customCards}
									>
										<Sparkles className="h-4 w-4" />
									</button>
								</div>
							</div>
							<div className="flex-1 min-h-0 overflow-y-auto p-4">
								{activeSidebarSection === "cards"
									? renderCardsManager()
									: renderCustomManager()}
							</div>
						</div>
					)}
				</div>
			</div>
		</div>
	);
}

export default DashboardApp;
