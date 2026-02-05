"use client";

import { useApp } from "@/hooks/use-app";
import { useIsMobile } from "@/hooks/use-mobile";
import { useCallback, useMemo, useState } from "react";
import {
	BuiltinCardRenderer,
	CustomMarkdownCard,
	DashboardGrid,
	DashboardHeader,
	DashboardSidebar,
	QueryCard,
} from "./components";
import { useDashboardData } from "./hooks";
import { getTranslations } from "./translations";
import type { BuiltinCardDefinition, DashboardCardSpan, DashboardRegistryCard } from "./types";

function createId(): string {
	if (typeof crypto !== "undefined" && crypto.randomUUID) {
		return crypto.randomUUID();
	}
	return Math.random().toString(36).slice(2);
}

export function DashboardApp() {
	const {
		locale,
		chatHistory,
		workspaceSessions,
		busySessions,
		selectedWorkspaceSession,
	} = useApp();

	const [rightSidebarCollapsed, setRightSidebarCollapsed] = useState(false);
	const [sidebarSection, setSidebarSection] = useState<"cards" | "custom">("cards");
	const [layoutEditMode, setLayoutEditMode] = useState(false);
	const isMobileLayout = useIsMobile();
	const [mobileView, setMobileView] = useState<"dashboard" | "cards" | "custom">("dashboard");

	const workspacePath = selectedWorkspaceSession?.workspace_path ?? ".";
	const t = useMemo(() => getTranslations(locale), [locale]);

	const builtinCards: BuiltinCardDefinition[] = useMemo(
		() => [
			{ id: "stat-sessions", title: "Sessions", defaultSpan: 3 },
			{ id: "stat-busy", title: "Busy Chats", defaultSpan: 3 },
			{ id: "stat-scheduler", title: "Scheduler", defaultSpan: 3 },
			{ id: "stat-trx", title: "TRX", defaultSpan: 3 },
			{ id: "scheduler", title: t.scheduler, defaultSpan: 6 },
			{ id: "agents", title: t.workingAgents, defaultSpan: 6 },
			{ id: "trx", title: t.trx, defaultSpan: 6 },
			{ id: "codexbar", title: "AI Subscriptions", defaultSpan: 6 },
			{ id: "feeds", title: t.feeds, defaultSpan: 6 },
		],
		[t],
	);

	const data = useDashboardData({
		workspacePath,
		configWorkspacePath: workspacePath,
		builtinCards,
	});

	const runningSessions = useMemo(
		() => workspaceSessions.filter((s) => s.status === "running"),
		[workspaceSessions],
	);

	const busyChatSessions = useMemo(() => {
		if (!busySessions.size) return [];
		return chatHistory.filter((s) => busySessions.has(s.id));
	}, [busySessions, chatHistory]);

	const cardMap = useMemo(() => {
		const map = new Map<string, BuiltinCardDefinition | DashboardRegistryCard>();
		for (const card of builtinCards) map.set(card.id, card);
		for (const card of data.registryCards) map.set(card.id, card);
		return map;
	}, [builtinCards, data.registryCards]);

	const orderedCards = useMemo(() => {
		if (!data.layoutConfig) return [];
		return data.layoutConfig.order
			.map((id) => cardMap.get(id))
			.filter(Boolean) as Array<BuiltinCardDefinition | DashboardRegistryCard>;
	}, [cardMap, data.layoutConfig]);

	const visibleCards = useMemo(() => {
		if (!data.layoutConfig) return [];
		return orderedCards.filter((c) => data.layoutConfig!.cards[c.id]?.visible !== false);
	}, [data.layoutConfig, orderedCards]);

	const handleToggleCard = useCallback(
		(id: string) => {
			data.updateLayout((prev) => ({
				...prev,
				cards: { ...prev.cards, [id]: { ...prev.cards[id], visible: !prev.cards[id]?.visible, span: prev.cards[id]?.span ?? 6 } },
			}));
		},
		[data],
	);

	const handleSpanChange = useCallback(
		(id: string, span: DashboardCardSpan) => {
			data.updateLayout((prev) => ({ ...prev, cards: { ...prev.cards, [id]: { ...prev.cards[id], span } } }));
		},
		[data],
	);

	const handleReorder = useCallback(
		(fromId: string, toId: string) => {
			data.updateLayout((prev) => {
				const order = [...prev.order];
				const fromIndex = order.indexOf(fromId);
				const toIndex = order.indexOf(toId);
				if (fromIndex === -1 || toIndex === -1) return prev;
				order.splice(fromIndex, 1);
				order.splice(toIndex, 0, fromId);
				return { ...prev, order };
			});
		},
		[data],
	);

	const handleAddFeed = useCallback(
		(url: string) => {
			if (!data.layoutConfig?.feeds?.includes(url)) {
				const next = [url, ...(data.layoutConfig?.feeds ?? [])].slice(0, 8);
				data.updateLayout((prev) => ({ ...prev, feeds: next }));
			}
		},
		[data],
	);

	const handleRemoveFeed = useCallback(
		(url: string) => {
			const next = (data.layoutConfig?.feeds ?? []).filter((e) => e !== url);
			data.updateLayout((prev) => ({ ...prev, feeds: next }));
		},
		[data],
	);

	const handleAddCustomCard = useCallback(
		(card: { title: string; description?: string; type: "markdown" | "query"; content?: string; url?: string; method?: string }) => {
			const id = `custom-${createId()}`;
			const newCard: DashboardRegistryCard = {
				id,
				title: card.title,
				description: card.description,
				kind: card.type,
				config: card.type === "markdown" ? { content: card.content } : { url: card.url, method: card.method },
			};
			const nextRegistry = [...data.registryRef.current, newCard];
			data.registryRef.current = nextRegistry;
			data.setRegistryCards(nextRegistry);
			void data.persistRegistry(nextRegistry);
			data.updateLayout((prev) => ({
				...prev,
				order: [...prev.order, newCard.id],
				cards: { ...prev.cards, [newCard.id]: { visible: true, span: 6 } },
			}));
		},
		[data],
	);

	const handleRemoveCustomCard = useCallback(
		(id: string) => {
			const nextRegistry = data.registryRef.current.filter((c) => c.id !== id);
			data.registryRef.current = nextRegistry;
			data.setRegistryCards(nextRegistry);
			void data.persistRegistry(nextRegistry);
			data.updateLayout((prev) => {
				const nextCards = { ...prev.cards };
				delete nextCards[id];
				return { ...prev, order: prev.order.filter((e) => e !== id), cards: nextCards };
			});
		},
		[data],
	);

	const scheduleList = data.scheduler?.schedules ?? [];
	const feedUrls = data.layoutConfig?.feeds ?? [];

	const renderBuiltinCard = useCallback(
		(id: string) => (
			<BuiltinCardRenderer
				cardId={id}
				locale={locale}
				translations={t}
				runningSessions={runningSessions}
				workspaceSessions={workspaceSessions}
				busyChatSessions={busyChatSessions}
				totalChatCount={chatHistory.length}
				scheduleList={scheduleList}
				scheduleStats={data.scheduleStats}
				schedulerError={data.schedulerError}
				schedulerLoading={data.schedulerLoading}
				onLoadScheduler={data.handleLoadScheduler}
				agents={data.agents}
				topTrxIssues={data.topTrxIssues}
				trxStats={data.trxStats}
				trxError={data.trxError}
				trxLoading={data.trxLoading}
				onLoadTrx={data.handleLoadTrx}
				codexbar={data.codexbar}
				onLoadCodexbar={data.handleLoadCodexbar}
				feedUrls={feedUrls}
				feeds={data.feeds}
				onAddFeed={handleAddFeed}
				onRemoveFeed={handleRemoveFeed}
				onRefreshFeeds={data.handleRefreshFeeds}
			/>
		),
		[locale, t, runningSessions, workspaceSessions, busyChatSessions, chatHistory.length, scheduleList, data, feedUrls, handleAddFeed, handleRemoveFeed],
	);

	const renderCustomCard = useCallback((card: DashboardRegistryCard) => {
		if (card.kind === "markdown") {
			return <CustomMarkdownCard title={card.title} description={card.description} content={card.config?.content || ""} />;
		}
		return <QueryCard title={card.title} description={card.description} url={card.config?.url} method={card.config?.method} headers={card.config?.headers} />;
	}, []);

	const activeSidebarSection = isMobileLayout && mobileView !== "dashboard" ? mobileView : sidebarSection;

	return (
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4 overflow-hidden w-full">
			{/* Mobile layout */}
			<div className="flex-1 min-h-0 flex flex-col lg:hidden">
				<DashboardHeader
					title={t.title}
					subtitle={t.subtitle}
					customCardsLabel={t.customCards}
					layoutEditMode={layoutEditMode}
					setLayoutEditMode={setLayoutEditMode}
					mobileView={mobileView}
					setMobileView={setMobileView}
					setSidebarSection={setSidebarSection}
					isMobile={true}
					layoutError={data.layoutError}
				/>
				<div className="flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-3 sm:p-4 overflow-hidden flex flex-col gap-4">
					<div className="flex-1 min-h-0 overflow-hidden">
						{mobileView === "dashboard" ? (
							<DashboardGrid
								layoutConfig={data.layoutConfig}
								layoutLoading={data.layoutLoading}
								layoutEditMode={layoutEditMode}
								visibleCards={visibleCards}
								renderBuiltinCard={renderBuiltinCard}
								renderCustomCard={renderCustomCard}
								onSpanChange={handleSpanChange}
								onReorder={handleReorder}
							/>
						) : (
							<div className="h-full overflow-y-auto">
								<DashboardSidebar
									layoutLabel={t.layout}
									noticeLabel={t.notice}
									customCardsLabel={t.customCards}
									collapsed={false}
									setCollapsed={() => {}}
									sidebarSection={activeSidebarSection as "cards" | "custom"}
									setSidebarSection={setSidebarSection}
									layoutConfig={data.layoutConfig}
									orderedCards={orderedCards}
									onToggleCard={handleToggleCard}
									onRemoveCustomCard={handleRemoveCustomCard}
									onAddCustomCard={handleAddCustomCard}
								/>
							</div>
						)}
					</div>
				</div>
			</div>

			{/* Desktop layout */}
			<div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
				<div className="flex-[3] min-w-0 bg-card border border-border p-4 xl:p-5 flex flex-col min-h-0 h-full">
					<DashboardHeader
						title={t.title}
						subtitle={t.subtitle}
						customCardsLabel={t.customCards}
						layoutEditMode={layoutEditMode}
						setLayoutEditMode={setLayoutEditMode}
						rightSidebarCollapsed={rightSidebarCollapsed}
						setRightSidebarCollapsed={setRightSidebarCollapsed}
						isMobile={false}
						layoutError={data.layoutError}
					/>
					<div className="flex-1 min-h-0 overflow-hidden mt-4">
						<DashboardGrid
							layoutConfig={data.layoutConfig}
							layoutLoading={data.layoutLoading}
							layoutEditMode={layoutEditMode}
							visibleCards={visibleCards}
							renderBuiltinCard={renderBuiltinCard}
							renderCustomCard={renderCustomCard}
							onSpanChange={handleSpanChange}
							onReorder={handleReorder}
						/>
					</div>
				</div>

				<DashboardSidebar
					layoutLabel={t.layout}
					noticeLabel={t.notice}
					customCardsLabel={t.customCards}
					collapsed={rightSidebarCollapsed}
					setCollapsed={setRightSidebarCollapsed}
					sidebarSection={sidebarSection}
					setSidebarSection={setSidebarSection}
					layoutConfig={data.layoutConfig}
					orderedCards={orderedCards}
					onToggleCard={handleToggleCard}
					onRemoveCustomCard={handleRemoveCustomCard}
					onAddCustomCard={handleAddCustomCard}
				/>
			</div>
		</div>
	);
}

export default DashboardApp;
