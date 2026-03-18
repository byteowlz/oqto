"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { type BusStats, useAdminBusStats } from "@/hooks/use-admin";
import { busSubscribe } from "@/lib/bus-client";
import type { BusEvent, BusScope } from "@/lib/ws-mux-types";
import {
	Activity,
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

const MAX_EVENTS = 200;

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
	const unsubscribeRef = useRef<(() => void) | null>(null);

	const topics = useMemo(
		() =>
			topicsInput
				.split(",")
				.map((topic) => topic.trim())
				.filter((topic) => topic.length > 0),
		[topicsInput],
	);

	const disconnect = useCallback(() => {
		if (unsubscribeRef.current) {
			unsubscribeRef.current();
			unsubscribeRef.current = null;
		}
		setConnected(false);
	}, []);

	const connect = useCallback(() => {
		disconnect();
		const subscription = busSubscribe(
			{
				scope,
				scopeId,
				topics: topics.length > 0 ? topics : ["**"],
			},
			(event) => {
				setEvents((prev) => {
					const next: EventRow = { ...event, receivedAt: Date.now() };
					const updated = [next, ...prev];
					if (updated.length > MAX_EVENTS) {
						return updated.slice(0, MAX_EVENTS);
					}
					return updated;
				});
			},
		);
		unsubscribeRef.current = subscription.unsubscribe;
		setConnected(true);
	}, [disconnect, scope, scopeId, topics]);

	useEffect(() => {
		connect();
		return () => disconnect();
	}, [connect, disconnect]);

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
				</div>
				<div className="flex items-center gap-2">
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

			<div className="grid grid-cols-1 md:grid-cols-3 gap-2">
				<label className="text-xs text-muted-foreground flex flex-col gap-1">
					Scope
					<select
						className="h-8 px-2 border border-border bg-background text-foreground"
						value={scope}
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
						className="h-8 px-2 border border-border bg-background text-foreground"
						value={scopeId}
						onChange={(e) => setScopeId(e.target.value)}
						placeholder={
							scope === "global" ? "global" : "workspace path / session id"
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
			</div>

			<div className="border border-border bg-background/40 max-h-96 overflow-auto">
				{events.length === 0 ? (
					<div className="px-3 py-4 text-xs text-muted-foreground">
						No events yet. Adjust scope/scope_id and click Connect.
					</div>
				) : (
					<ul className="divide-y divide-border">
						{events.map((event) => (
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
