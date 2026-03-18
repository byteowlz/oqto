"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { type BusStats, useAdminBusStats } from "@/hooks/use-admin";
import { busSubscribe } from "@/lib/bus-client";
import type { BusEvent, BusScope } from "@/lib/ws-mux-types";
import {
	Activity,
	Download,
	Pause,
	Play,
	Radio,
	RefreshCw,
	Trash2,
	Wifi,
	WifiOff,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

type EventRow = BusEvent & {
	receivedAt: number;
};

type TopicPreset = {
	label: string;
	scope: BusScope;
	scopeId: string;
	topics: string;
};

const MAX_EVENTS = 200;

const TOPIC_PRESETS: TopicPreset[] = [
	{ label: "All (global)", scope: "global", scopeId: "global", topics: "**" },
	{
		label: "All workspaces (admin)",
		scope: "workspace",
		scopeId: "*",
		topics: "**",
	},
	{
		label: "Session lifecycle",
		scope: "workspace",
		scopeId: "*",
		topics: "session.**",
	},
	{
		label: "Files",
		scope: "workspace",
		scopeId: "*",
		topics: "files.**",
	},
	{
		label: "Services",
		scope: "global",
		scopeId: "global",
		topics: "service.**",
	},
];

function metric(label: string, value: string | number) {
	return (
		<div className="bg-muted/30 border border-border px-3 py-2">
			<div className="text-[10px] uppercase tracking-wider text-muted-foreground">
				{label}
			</div>
			<div className="text-sm font-mono text-foreground">{value}</div>
		</div>
	);
}

function renderSource(source: BusEvent["source"]): string {
	switch (source.type) {
		case "service":
			return `service:${source.service}`;
		case "admin":
			return `admin:${source.user_id}`;
		case "backend":
			return "backend";
		case "runner":
			return `runner:${source.runner_id}`;
		case "agent":
			return `agent:${source.runner_id}`;
		case "frontend":
			return `frontend:${source.user_id}`;
		case "app":
			return `app:${source.app_id}`;
		default:
			return source.type;
	}
}

function pretty(value: unknown): string {
	try {
		return JSON.stringify(value, null, 2);
	} catch {
		return String(value);
	}
}

export function EventBusPanel() {
	const { data: stats, isLoading, error, refetch } = useAdminBusStats();
	const [scope, setScope] = useState<BusScope>("global");
	const [scopeId, setScopeId] = useState("global");
	const [topicsInput, setTopicsInput] = useState("**");
	const [events, setEvents] = useState<EventRow[]>([]);
	const [connected, setConnected] = useState(false);
	const [paused, setPaused] = useState(false);
	const [adminGlobalView, setAdminGlobalView] = useState(true);
	const [filterText, setFilterText] = useState("");
	const [autoExportEnabled, setAutoExportEnabled] = useState(false);
	const [autoExportEvery, setAutoExportEvery] = useState("50");
	const autoExportNextThresholdRef = useRef(50);
	const unsubscribeRef = useRef<Array<() => void>>([]);
	const pausedRef = useRef(paused);

	useEffect(() => {
		pausedRef.current = paused;
	}, [paused]);

	const topics = useMemo(
		() =>
			topicsInput
				.split(",")
				.map((topic) => topic.trim())
				.filter((topic) => topic.length > 0),
		[topicsInput],
	);

	const parsedAutoExportEvery = useMemo(() => {
		const parsed = Number.parseInt(autoExportEvery, 10);
		return Number.isFinite(parsed) && parsed > 0 ? parsed : 50;
	}, [autoExportEvery]);

	const filteredEvents = useMemo(() => {
		const needle = filterText.trim().toLowerCase();
		if (needle.length === 0) return events;
		return events.filter((event) => {
			if (event.topic.toLowerCase().includes(needle)) return true;
			if (event.scope_id.toLowerCase().includes(needle)) return true;
			if (renderSource(event.source).toLowerCase().includes(needle))
				return true;
			const payloadText = pretty(event.payload).toLowerCase();
			return payloadText.includes(needle);
		});
	}, [events, filterText]);

	const disconnect = useCallback(() => {
		for (const unsub of unsubscribeRef.current) {
			unsub();
		}
		unsubscribeRef.current = [];
		setConnected(false);
	}, []);

	const connect = useCallback(() => {
		disconnect();

		const onEvent = (event: BusEvent) => {
			if (pausedRef.current) return;
			setEvents((prev) => {
				const next: EventRow = { ...event, receivedAt: Date.now() };
				const updated = [next, ...prev];
				if (updated.length > MAX_EVENTS) {
					return updated.slice(0, MAX_EVENTS);
				}
				return updated;
			});
		};

		const subs: Array<() => void> = [];
		if (adminGlobalView) {
			subs.push(
				busSubscribe(
					{
						scope: "global",
						scopeId: "global",
						topics: topics.length > 0 ? topics : ["**"],
					},
					onEvent,
				).unsubscribe,
			);
			subs.push(
				busSubscribe(
					{
						scope: "workspace",
						scopeId: "*",
						topics: topics.length > 0 ? topics : ["**"],
					},
					onEvent,
				).unsubscribe,
			);
			subs.push(
				busSubscribe(
					{
						scope: "session",
						scopeId: "*",
						topics: topics.length > 0 ? topics : ["**"],
					},
					onEvent,
				).unsubscribe,
			);
		} else {
			subs.push(
				busSubscribe(
					{
						scope,
						scopeId,
						topics: topics.length > 0 ? topics : ["**"],
					},
					onEvent,
				).unsubscribe,
			);
		}

		unsubscribeRef.current = subs;
		setConnected(true);
	}, [adminGlobalView, disconnect, scope, scopeId, topics]);

	useEffect(() => {
		connect();
		return () => disconnect();
	}, [connect, disconnect]);

	const exportEvents = useCallback(
		(rows: EventRow[], suffix: "all" | "filtered") => {
			const dump = {
				exported_at: new Date().toISOString(),
				scope,
				scope_id: scopeId,
				topics: topics.length > 0 ? topics : ["**"],
				filter_text: filterText.trim() || undefined,
				event_count: rows.length,
				events: rows,
			};
			const blob = new Blob([JSON.stringify(dump, null, 2)], {
				type: "application/json",
			});
			const url = URL.createObjectURL(blob);
			const link = document.createElement("a");
			link.href = url;
			link.download = `oqto-eventbus-${suffix}-${Date.now()}.json`;
			document.body.appendChild(link);
			link.click();
			document.body.removeChild(link);
			URL.revokeObjectURL(url);
		},
		[filterText, scope, scopeId, topics],
	);

	useEffect(() => {
		autoExportNextThresholdRef.current = parsedAutoExportEvery;
	}, [parsedAutoExportEvery]);

	useEffect(() => {
		if (!autoExportEnabled) return;
		if (events.length < autoExportNextThresholdRef.current) return;
		exportEvents(events, "all");
		autoExportNextThresholdRef.current += parsedAutoExportEvery;
	}, [autoExportEnabled, events, exportEvents, parsedAutoExportEvery]);

	const latestEvent = events[0];

	return (
		<div className="space-y-4 border border-border p-4 bg-card">
			<div className="flex flex-wrap items-center justify-between gap-2">
				<div className="flex items-center gap-2">
					<Radio className="w-4 h-4 text-primary" />
					<h3 className="text-sm font-semibold tracking-wide">Event Bus</h3>
					{connected ? (
						<Badge variant="secondary" className="gap-1">
							<Wifi className="w-3 h-3" /> live
						</Badge>
					) : (
						<Badge variant="outline" className="gap-1">
							<WifiOff className="w-3 h-3" /> disconnected
						</Badge>
					)}
					{paused && (
						<Badge variant="outline" className="gap-1">
							<Pause className="w-3 h-3" /> paused
						</Badge>
					)}
				</div>
				<div className="flex items-center gap-2">
					<Button
						type="button"
						size="sm"
						variant={adminGlobalView ? "default" : "outline"}
						onClick={() => setAdminGlobalView((prev) => !prev)}
					>
						{adminGlobalView ? "Admin global view" : "Scoped view"}
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						onClick={() => {
							void refetch();
						}}
					>
						<RefreshCw className="w-3.5 h-3.5" />
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						onClick={() => setPaused((prev) => !prev)}
					>
						{paused ? (
							<Play className="w-3.5 h-3.5" />
						) : (
							<Pause className="w-3.5 h-3.5" />
						)}
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						onClick={() => exportEvents(events, "all")}
						disabled={events.length === 0}
					>
						<Download className="w-3.5 h-3.5" />
						<span className="ml-1">All</span>
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						onClick={() => exportEvents(filteredEvents, "filtered")}
						disabled={filteredEvents.length === 0}
					>
						<Download className="w-3.5 h-3.5" />
						<span className="ml-1">Filtered</span>
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						onClick={() => setEvents([])}
					>
						<Trash2 className="w-3.5 h-3.5" />
					</Button>
					{connected ? (
						<Button
							type="button"
							size="sm"
							variant="outline"
							onClick={disconnect}
						>
							Disconnect
						</Button>
					) : (
						<Button type="button" size="sm" onClick={connect}>
							Connect
						</Button>
					)}
				</div>
			</div>

			<div className="flex flex-wrap gap-2">
				{TOPIC_PRESETS.map((preset) => (
					<Button
						key={preset.label}
						type="button"
						size="sm"
						variant="outline"
						onClick={() => {
							setScope(preset.scope);
							setScopeId(preset.scopeId);
							setTopicsInput(preset.topics);
						}}
					>
						{preset.label}
					</Button>
				))}
			</div>

			<div className="grid grid-cols-1 md:grid-cols-3 gap-2">
				<label className="text-xs text-muted-foreground flex flex-col gap-1">
					Scope
					<select
						className="h-8 px-2 border border-border bg-background text-foreground disabled:opacity-50"
						value={scope}
						disabled={adminGlobalView}
						onChange={(e) => {
							const nextScope = e.target.value as BusScope;
							setScope(nextScope);
							if (nextScope === "global") setScopeId("global");
						}}
					>
						<option value="global">global</option>
						<option value="workspace">workspace</option>
						<option value="session">session</option>
					</select>
				</label>
				<label className="text-xs text-muted-foreground flex flex-col gap-1">
					Scope ID
					<input
						className="h-8 px-2 border border-border bg-background text-foreground disabled:opacity-50"
						value={scopeId}
						disabled={adminGlobalView}
						onChange={(e) => setScopeId(e.target.value)}
						placeholder={
							adminGlobalView
								? "auto: global + workspace:* + session:*"
								: scope === "global"
									? "global"
									: "workspace path / session id"
						}
					/>
				</label>
				<label className="text-xs text-muted-foreground flex flex-col gap-1">
					Topics (comma separated globs)
					<input
						className="h-8 px-2 border border-border bg-background text-foreground"
						value={topicsInput}
						onChange={(e) => setTopicsInput(e.target.value)}
						placeholder="**"
					/>
				</label>
			</div>

			<div className="grid grid-cols-1 md:grid-cols-3 gap-2">
				<label className="text-xs text-muted-foreground flex flex-col gap-1">
					Filter events (topic/source/scope/payload)
					<input
						className="h-8 px-2 border border-border bg-background text-foreground"
						value={filterText}
						onChange={(e) => setFilterText(e.target.value)}
						placeholder="session.created"
					/>
				</label>
				<label className="text-xs text-muted-foreground flex flex-col gap-1">
					Auto-export every N events
					<input
						className="h-8 px-2 border border-border bg-background text-foreground"
						value={autoExportEvery}
						onChange={(e) => setAutoExportEvery(e.target.value)}
						placeholder="50"
					/>
				</label>
				<div className="text-xs text-muted-foreground flex flex-col gap-1">
					<span>Auto-export</span>
					<Button
						type="button"
						size="sm"
						variant={autoExportEnabled ? "default" : "outline"}
						onClick={() => setAutoExportEnabled((prev) => !prev)}
					>
						{autoExportEnabled ? "Enabled" : "Disabled"}
					</Button>
				</div>
			</div>

			<div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-2">
				{metric(
					"subscribers",
					(stats as BusStats | undefined)?.subscriber_count ?? "-",
				)}
				{metric(
					"subscriptions",
					(stats as BusStats | undefined)?.total_subscriptions ?? "-",
				)}
				{metric(
					"published",
					(stats as BusStats | undefined)?.events_published ?? "-",
				)}
				{metric(
					"delivered",
					(stats as BusStats | undefined)?.events_delivered ?? "-",
				)}
				{metric(
					"dropped authz",
					(stats as BusStats | undefined)?.events_dropped_authz ?? "-",
				)}
				{metric(
					"dropped rate",
					(stats as BusStats | undefined)?.events_dropped_rate ?? "-",
				)}
			</div>

			<div className="text-xs text-muted-foreground flex items-center gap-2">
				<Activity className="w-3.5 h-3.5" />
				{isLoading
					? "Loading bus stats..."
					: error
						? `Stats error: ${error instanceof Error ? error.message : String(error)}`
						: latestEvent
							? `Last event: ${latestEvent.topic} (${new Date(latestEvent.ts).toLocaleTimeString()})`
							: "No events received yet"}
				<span className="font-mono">
					showing {filteredEvents.length}/{events.length}
				</span>
			</div>

			<div className="border border-border bg-background/40 max-h-96 overflow-auto">
				{filteredEvents.length === 0 ? (
					<div className="px-3 py-4 text-xs text-muted-foreground">
						No matching events yet. Adjust filters/topics and click Connect.
					</div>
				) : (
					<ul className="divide-y divide-border">
						{filteredEvents.map((event) => (
							<li
								key={`${event.event_id}-${event.receivedAt}`}
								className="px-3 py-2"
							>
								<div className="flex flex-wrap items-center gap-2 text-xs">
									<Badge variant="outline" className="font-mono">
										{event.topic}
									</Badge>
									<span className="font-mono text-muted-foreground">
										{event.scope}/{event.scope_id}
									</span>
									<span className="text-muted-foreground">
										{renderSource(event.source)}
									</span>
									<span className="text-muted-foreground">
										{new Date(event.ts).toLocaleTimeString()}
									</span>
								</div>
								<details className="mt-1">
									<summary className="cursor-pointer text-[11px] text-muted-foreground">
										payload
									</summary>
									<pre className="mt-1 text-[11px] leading-4 overflow-auto bg-background border border-border p-2">
										{pretty(event.payload)}
									</pre>
								</details>
							</li>
						))}
					</ul>
				)}
			</div>
		</div>
	);
}
