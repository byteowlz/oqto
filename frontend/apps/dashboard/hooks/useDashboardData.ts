import {
	type CodexBarUsagePayload,
	type SchedulerOverview,
	fetchFeed,
	getCodexBarUsage,
	deleteSchedulerJob,
	getSchedulerOverview,
} from "@/lib/control-plane-client";
import { readFileMux, writeFileMux } from "@/lib/mux-files";
import type { AgentInfo } from "@/lib/agent-client";
import { fetchAgents } from "@/lib/agent-client";
import { getWsManager } from "@/lib/ws-manager";
import type { TrxWsEvent } from "@/lib/ws-mux-types";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
	BuiltinCardDefinition,
	CodexBarState,
	DashboardCardSpan,
	DashboardLayoutConfig,
	DashboardRegistryCard,
	DashboardRegistryConfig,
	FeedState,
	TrxIssue,
} from "../types";

const DASHBOARD_CONFIG_PATH = ".octo/dashboard.json";
const DASHBOARD_REGISTRY_PATH = ".octo/dashboard.registry.json";
const LEGACY_FEED_STORAGE_KEY = "octo:dashboardFeeds";

function createId(): string {
	if (typeof crypto !== "undefined" && crypto.randomUUID) {
		return crypto.randomUUID();
	}
	return Math.random().toString(36).slice(2);
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
	const manager = getWsManager();
	manager.connect();
	const response = (await manager.sendAndWait({
		channel: "trx",
		type: "list",
		workspace_path: workspacePath,
	})) as TrxWsEvent;

	if (response.type === "list_result") {
		return response.issues as TrxIssue[];
	}
	if (response.type === "error") {
		const message = response.error.toLowerCase();
		if (
			message.includes("not initialized") ||
			message.includes("no .trx") ||
			message.includes("404")
		) {
			return [];
		}
		throw new Error(response.error);
	}
	throw new Error("Failed to fetch TRX issues");
}

async function readWorkspaceFile(
	workspacePath: string,
	path: string,
): Promise<string | null> {
	try {
		const result = await readFileMux(workspacePath, path);
		const decoder = new TextDecoder("utf-8");
		return decoder.decode(result.data);
	} catch {
		return null;
	}
}

async function writeWorkspaceFile(
	workspacePath: string,
	path: string,
	content: string,
): Promise<void> {
	const encoder = new TextEncoder();
	await writeFileMux(workspacePath, path, encoder.encode(content).buffer, true);
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
	const cards: Record<string, { visible: boolean; span: DashboardCardSpan }> =
		{};
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

	const nextCards: Record<
		string,
		{ visible: boolean; span: DashboardCardSpan }
	> = {
		...layout.cards,
	};
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

export type UseDashboardDataOptions = {
	workspacePath: string;
	configWorkspacePath: string;
	agentBaseUrl?: string;
	agentDirectory?: string;
	builtinCards: BuiltinCardDefinition[];
};

export type UseDashboardDataReturn = {
	// Scheduler
	scheduler: SchedulerOverview | null;
	schedulerError: string | null;
	schedulerLoading: boolean;
	handleLoadScheduler: () => Promise<void>;
	handleDeleteSchedulerJob: (name: string) => Promise<void>;
	scheduleStats: { total: number; enabled: number; disabled: number };

	// Agents
	agents: AgentInfo[];
	handleLoadAgents: () => Promise<void>;

	// TRX
	trxIssues: TrxIssue[];
	trxError: string | null;
	trxLoading: boolean;
	handleLoadTrx: () => Promise<void>;
	trxStats: {
		total: number;
		open: number;
		inProgress: number;
		blocked: number;
	};
	topTrxIssues: TrxIssue[];

	// Feeds
	feeds: Record<string, FeedState>;
	loadFeed: (url: string) => Promise<void>;
	handleRefreshFeeds: (feedUrls: string[]) => void;

	// CodexBar
	codexbar: CodexBarState;
	handleLoadCodexbar: () => Promise<void>;

	// Layout
	layoutConfig: DashboardLayoutConfig | null;
	layoutError: string | null;
	layoutLoading: boolean;
	updateLayout: (
		updater: (layout: DashboardLayoutConfig) => DashboardLayoutConfig,
	) => void;
	registryCards: DashboardRegistryCard[];
	persistRegistry: (next: DashboardRegistryCard[]) => Promise<void>;
	setRegistryCards: React.Dispatch<
		React.SetStateAction<DashboardRegistryCard[]>
	>;
	registryRef: React.MutableRefObject<DashboardRegistryCard[]>;
};

export function useDashboardData(
	options: UseDashboardDataOptions,
): UseDashboardDataReturn {
	const {
		workspacePath,
		configWorkspacePath,
		agentBaseUrl,
		agentDirectory,
		builtinCards,
	} = options;

	const [scheduler, setScheduler] = useState<SchedulerOverview | null>(null);
	const [schedulerError, setSchedulerError] = useState<string | null>(null);
	const [schedulerLoading, setSchedulerLoading] = useState(false);
	const [agents, setAgents] = useState<AgentInfo[]>([]);
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

	const builtinDefaultSpans = useMemo(() => {
		return builtinCards.reduce<Record<string, DashboardCardSpan>>(
			(acc, card) => {
				acc[card.id] = card.defaultSpan;
				return acc;
			},
			{},
		);
	}, [builtinCards]);

	const scheduleStats = scheduler?.stats ?? {
		total: 0,
		enabled: 0,
		disabled: 0,
	};

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

	const topTrxIssues = useMemo(() => {
		return [...trxIssues]
			.sort((a, b) => b.updated_at.localeCompare(a.updated_at))
			.slice(0, 6);
	}, [trxIssues]);

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

	const handleDeleteSchedulerJob = useCallback(
		async (name: string) => {
			await deleteSchedulerJob(name);
			// Reload the list after deletion
			await handleLoadScheduler();
		},
		[handleLoadScheduler],
	);

	const handleLoadAgents = useCallback(async () => {
		if (!agentBaseUrl) return;
		try {
			const list = await fetchAgents(agentBaseUrl, {
				directory: agentDirectory,
			});
			setAgents(list);
		} catch (err) {
			console.error("Failed to fetch agents:", err);
			setAgents([]);
		}
	}, [agentBaseUrl, agentDirectory]);

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

	return {
		scheduler,
		schedulerError,
		schedulerLoading,
		handleLoadScheduler,
		handleDeleteSchedulerJob,
		scheduleStats,
		agents,
		handleLoadAgents,
		trxIssues,
		trxError,
		trxLoading,
		handleLoadTrx,
		trxStats,
		topTrxIssues,
		feeds,
		loadFeed,
		handleRefreshFeeds,
		codexbar,
		handleLoadCodexbar,
		layoutConfig,
		layoutError,
		layoutLoading,
		updateLayout,
		registryCards,
		persistRegistry,
		setRegistryCards,
		registryRef,
	};
}
