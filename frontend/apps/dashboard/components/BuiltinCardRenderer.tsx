import type { OpenCodeAgent, OpenCodeSession } from "@/lib/opencode-client";
import type { WorkspaceSession } from "@/lib/workspace-session";
import { Activity, CalendarClock, Flame, ListTodo } from "lucide-react";
import { memo } from "react";
import type { CodexBarState, FeedState, TrxIssue } from "../types";
import { ActiveAgentsCard } from "./ActiveAgentsCard";
import { CodexBarCard } from "./CodexBarCard";
import { FeedsCard } from "./FeedsCard";
import { SchedulerCard } from "./SchedulerCard";
import { StatCard } from "./StatCard";
import { TrxCard } from "./TrxCard";

type ScheduleItem = {
	name: string;
	status: string;
	command: string;
	schedule: string;
	next_run?: string | null;
};

export type BuiltinCardRendererProps = {
	cardId: string;
	locale: "de" | "en";
	translations: {
		stats: string;
		scheduler: string;
		workingAgents: string;
		trx: string;
		feeds: string;
		addFeed: string;
		reload: string;
		noTasks: string;
		noAgents: string;
		noTrx: string;
		noFeeds: string;
	};
	// Session data
	runningSessions: WorkspaceSession[];
	workspaceSessions: WorkspaceSession[];
	busyChatSessions: OpenCodeSession[];
	opencodeSessions: OpenCodeSession[];
	// Scheduler data
	scheduleList: ScheduleItem[];
	scheduleStats: { total: number; enabled: number; disabled: number };
	schedulerError: string | null;
	schedulerLoading: boolean;
	onLoadScheduler: () => void;
	// Agent data
	agents: OpenCodeAgent[];
	// TRX data
	topTrxIssues: TrxIssue[];
	trxStats: {
		total: number;
		open: number;
		inProgress: number;
		blocked: number;
	};
	trxError: string | null;
	trxLoading: boolean;
	onLoadTrx: () => void;
	// CodexBar data
	codexbar: CodexBarState;
	onLoadCodexbar: () => void;
	// Feed data
	feedUrls: string[];
	feeds: Record<string, FeedState>;
	onAddFeed: (url: string) => void;
	onRemoveFeed: (url: string) => void;
	onRefreshFeeds: (urls: string[]) => void;
};

export const BuiltinCardRenderer = memo(function BuiltinCardRenderer({
	cardId,
	locale,
	translations: t,
	runningSessions,
	workspaceSessions,
	busyChatSessions,
	opencodeSessions,
	scheduleList,
	scheduleStats,
	schedulerError,
	schedulerLoading,
	onLoadScheduler,
	agents,
	topTrxIssues,
	trxStats,
	trxError,
	trxLoading,
	onLoadTrx,
	codexbar,
	onLoadCodexbar,
	feedUrls,
	feeds,
	onAddFeed,
	onRemoveFeed,
	onRefreshFeeds,
}: BuiltinCardRendererProps) {
	switch (cardId) {
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
				<SchedulerCard
					title={t.scheduler}
					reloadLabel={t.reload}
					noTasksLabel={t.noTasks}
					scheduleList={scheduleList}
					scheduleStats={scheduleStats}
					schedulerError={schedulerError}
					schedulerLoading={schedulerLoading}
					locale={locale}
					onReload={onLoadScheduler}
				/>
			);
		case "agents":
			return (
				<ActiveAgentsCard
					title={t.workingAgents}
					noAgentsLabel={t.noAgents}
					runningSessions={runningSessions}
					busyChatSessions={busyChatSessions}
					agents={agents}
				/>
			);
		case "trx":
			return (
				<TrxCard
					title={t.trx}
					reloadLabel={t.reload}
					noTrxLabel={t.noTrx}
					topIssues={topTrxIssues}
					trxStats={trxStats}
					trxError={trxError}
					trxLoading={trxLoading}
					onReload={onLoadTrx}
				/>
			);
		case "codexbar":
			return (
				<CodexBarCard
					reloadLabel={t.reload}
					codexbar={codexbar}
					onReload={onLoadCodexbar}
				/>
			);
		case "feeds":
			return (
				<FeedsCard
					title={t.feeds}
					reloadLabel={t.reload}
					addFeedLabel={t.addFeed}
					noFeedsLabel={t.noFeeds}
					feedUrls={feedUrls}
					feeds={feeds}
					onAddFeed={onAddFeed}
					onRemoveFeed={onRemoveFeed}
					onRefreshFeeds={onRefreshFeeds}
				/>
			);
		default:
			return null;
	}
});
