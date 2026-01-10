"use client";

import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import {
	type AdminMetricsSnapshot,
	type HostMetrics,
	useAdminMetrics,
} from "@/hooks/use-admin";
import {
	Activity,
	AlertTriangle,
	Cpu,
	HardDrive,
	Network,
	Server,
	Wifi,
	WifiOff,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";

function formatBytes(bytes: number): string {
	if (bytes === 0) return "0 B";
	const k = 1024;
	const sizes = ["B", "KB", "MB", "GB", "TB"];
	const i = Math.floor(Math.log(bytes) / Math.log(k));
	return `${Number.parseFloat((bytes / k ** i).toFixed(1))} ${sizes[i]}`;
}

function StatCard({
	label,
	value,
	Icon,
	percent,
	subValue,
}: {
	label: string;
	value: string | number;
	Icon: React.ElementType;
	percent?: number;
	subValue?: string;
}) {
	return (
		<div className="bg-card border border-border p-3 md:p-4 flex flex-col gap-2 md:gap-3 hover:border-primary transition">
			<div className="flex items-center justify-between">
				<div className="min-w-0">
					<p className="text-[10px] md:text-xs text-muted-foreground tracking-wider truncate">
						{label}
					</p>
					<p className="text-lg md:text-2xl font-bold text-foreground font-mono">
						{value}
					</p>
					{subValue && (
						<p className="text-[10px] text-muted-foreground">{subValue}</p>
					)}
				</div>
				<Icon className="w-6 h-6 md:w-8 md:h-8 shrink-0 text-primary" />
			</div>
			{percent !== undefined && (
				<div className="h-1.5 md:h-2 bg-muted overflow-hidden">
					<div
						className="h-full bg-primary/70 transition-all duration-500"
						style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
					/>
				</div>
			)}
		</div>
	);
}

type DataPoint = {
	timestamp: number;
	cpu: number;
	mem: number;
};

const MAX_POINTS = 60; // Last 2 minutes at 2-second intervals

function MetricsChart({ dataPoints }: { dataPoints: DataPoint[] }) {
	if (dataPoints.length < 2) {
		return (
			<div className="h-32 md:h-48 flex items-center justify-center text-muted-foreground text-sm">
				Collecting data...
			</div>
		);
	}

	// Create SVG path for the line chart
	const width = 480;
	const height = 150;
	const padding = { top: 10, right: 10, bottom: 10, left: 10 };

	const innerWidth = width - padding.left - padding.right;
	const innerHeight = height - padding.top - padding.bottom;

	const createPath = (values: number[]): string => {
		if (values.length < 2) return "";

		const xStep = innerWidth / (MAX_POINTS - 1);
		const points = values.map((val, i) => {
			const x = padding.left + i * xStep;
			const y = padding.top + innerHeight - (val / 100) * innerHeight;
			return `${x},${y}`;
		});

		return `M ${points.join(" L ")}`;
	};

	const cpuValues = dataPoints.map((p) => p.cpu);
	const memValues = dataPoints.map((p) => p.mem);

	const cpuPath = createPath(cpuValues);
	const memPath = createPath(memValues);

	return (
		<div className="h-32 md:h-48 relative">
			{/* Grid */}
			<div className="absolute inset-0 grid grid-cols-6 md:grid-cols-12 grid-rows-4 md:grid-rows-6 opacity-20">
				{Array.from({ length: 24 }, (_, i) => `grid-${i}`).map((key) => (
					<div key={key} className="border border-border" />
				))}
			</div>

			{/* Chart */}
			<svg
				className="absolute inset-0 w-full h-full"
				viewBox={`0 0 ${width} ${height}`}
				preserveAspectRatio="none"
			>
				<title>System metrics chart</title>
				{/* CPU line */}
				<path
					d={cpuPath}
					fill="none"
					stroke="var(--primary)"
					strokeWidth="2"
					vectorEffect="non-scaling-stroke"
				/>
				{/* Memory line */}
				<path
					d={memPath}
					fill="none"
					stroke="var(--foreground)"
					strokeWidth="2"
					strokeDasharray="5,5"
					vectorEffect="non-scaling-stroke"
					opacity="0.5"
				/>
			</svg>

			{/* Legend */}
			<div className="absolute top-2 right-2 flex gap-3 md:gap-4 text-[10px] md:text-xs">
				<div className="flex items-center gap-1">
					<div className="w-2 md:w-3 h-0.5 bg-primary" />
					<span className="text-muted-foreground">CPU</span>
				</div>
				<div className="flex items-center gap-1">
					<div className="w-2 md:w-3 h-0.5 bg-foreground opacity-50" />
					<span className="text-muted-foreground">Memory</span>
				</div>
			</div>

			{/* Current values */}
			<div className="absolute bottom-2 left-2 flex gap-4 text-xs">
				<span className="text-primary font-mono">
					CPU: {cpuValues[cpuValues.length - 1]?.toFixed(1) ?? 0}%
				</span>
				<span className="text-muted-foreground font-mono">
					MEM: {memValues[memValues.length - 1]?.toFixed(1) ?? 0}%
				</span>
			</div>
		</div>
	);
}

export function MetricsPanel() {
	const { metrics, error, isConnected } = useAdminMetrics();
	const [dataPoints, setDataPoints] = useState<DataPoint[]>([]);

	// Accumulate data points for the chart
	useEffect(() => {
		if (!metrics?.host) return;

		const memPercent =
			(metrics.host.mem_used_bytes / metrics.host.mem_total_bytes) * 100;

		setDataPoints((prev) => {
			const newPoint: DataPoint = {
				timestamp: Date.now(),
				cpu: metrics.host?.cpu_percent ?? 0,
				mem: memPercent,
			};

			const updated = [...prev, newPoint];
			// Keep only the last MAX_POINTS
			if (updated.length > MAX_POINTS) {
				return updated.slice(-MAX_POINTS);
			}
			return updated;
		});
	}, [metrics]);

	const host = metrics?.host;
	const activeSessions = metrics?.containers?.length ?? 0;

	const cpuPercent = host?.cpu_percent ?? 0;
	const memPercent = host
		? (host.mem_used_bytes / host.mem_total_bytes) * 100
		: 0;

	return (
		<div className="space-y-4">
			{/* Connection Status */}
			<div className="flex items-center gap-2 text-xs">
				{isConnected ? (
					<>
						<Wifi className="w-3 h-3 text-green-500" />
						<span className="text-muted-foreground">
							Live metrics connected
						</span>
					</>
				) : (
					<>
						<WifiOff className="w-3 h-3 text-destructive" />
						<span className="text-destructive">
							{error || "Connecting to metrics stream..."}
						</span>
					</>
				)}
			</div>

			{/* Stats Grid */}
			<div className="grid grid-cols-2 lg:grid-cols-4 gap-3 md:gap-4 w-full">
				<StatCard
					label="CPU USAGE"
					value={`${cpuPercent.toFixed(1)}%`}
					Icon={Cpu}
					percent={cpuPercent}
				/>
				<StatCard
					label="MEMORY"
					value={host ? formatBytes(host.mem_used_bytes) : "-"}
					Icon={HardDrive}
					percent={memPercent}
					subValue={host ? `/ ${formatBytes(host.mem_total_bytes)}` : undefined}
				/>
				<StatCard
					label="ACTIVE SESSIONS"
					value={activeSessions}
					Icon={Activity}
					percent={100}
				/>
				<StatCard
					label="RUNTIME"
					value={activeSessions > 0 ? "Active" : "Idle"}
					Icon={Server}
					percent={activeSessions > 0 ? 100 : 0}
				/>
			</div>

			{/* Real-time Chart */}
			<div className="bg-card border border-border">
				<div className="border-b border-border px-3 md:px-4 py-2 md:py-3">
					<h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">
						REAL-TIME SYSTEM METRICS
					</h2>
				</div>
				<div className="p-3 md:p-4">
					<MetricsChart dataPoints={dataPoints} />
				</div>
			</div>

			{/* Container Stats */}
			{metrics?.containers && metrics.containers.length > 0 && (
				<div className="bg-card border border-border">
					<div className="border-b border-border px-3 md:px-4 py-2 md:py-3">
						<h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">
							CONTAINER RESOURCE USAGE
						</h2>
					</div>
					<div className="p-3 md:p-4 space-y-3">
						{metrics.containers.map((container) => (
							<div
								key={container.session_id}
								className="border border-border p-3 space-y-2"
							>
								<div className="flex items-center justify-between">
									<div className="min-w-0">
										<code className="text-xs text-foreground">
											{container.container_name}
										</code>
										<p className="text-[10px] text-muted-foreground truncate">
											{container.session_id.slice(0, 8)}...
										</p>
									</div>
									<Badge variant="outline" className="text-[10px]">
										{container.stats.pids} PIDs
									</Badge>
								</div>

								<div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-xs">
									<div className="flex items-center gap-1">
										<Cpu className="w-3 h-3 text-primary" />
										<span className="text-muted-foreground">CPU:</span>
										<span className="font-mono">
											{container.stats.cpu_percent}
										</span>
									</div>
									<div className="flex items-center gap-1">
										<HardDrive className="w-3 h-3 text-muted-foreground" />
										<span className="text-muted-foreground">MEM:</span>
										<span className="font-mono">
											{container.stats.mem_usage}
										</span>
									</div>
									<div className="flex items-center gap-1">
										<Network className="w-3 h-3 text-muted-foreground" />
										<span className="text-muted-foreground">NET:</span>
										<span className="font-mono">{container.stats.net_io}</span>
									</div>
									<div className="flex items-center gap-1">
										<HardDrive className="w-3 h-3 text-muted-foreground" />
										<span className="text-muted-foreground">I/O:</span>
										<span className="font-mono">
											{container.stats.block_io}
										</span>
									</div>
								</div>
							</div>
						))}
					</div>
				</div>
			)}

			{/* Errors */}
			{metrics?.error && (
				<div className="bg-destructive/10 border border-destructive/20 p-3 text-sm text-destructive flex items-center gap-2">
					<AlertTriangle className="w-4 h-4 shrink-0" />
					<span>{metrics.error}</span>
				</div>
			)}
		</div>
	);
}
