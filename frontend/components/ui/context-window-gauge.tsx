"use client";

import { Gauge } from "lucide-react";
import { cn } from "@/lib/utils";

export function ContextWindowGauge({
	inputTokens,
	outputTokens,
	maxTokens = 200000,
	locale,
	compact = false,
}: {
	inputTokens: number;
	outputTokens: number;
	maxTokens?: number;
	locale: "de" | "en";
	compact?: boolean;
}) {
	const totalTokens = inputTokens + outputTokens;
	const percentage = Math.min((totalTokens / maxTokens) * 100, 100);

	const getColor = () => {
		if (percentage >= 90) return "bg-destructive";
		if (percentage >= 70) return "bg-yellow-500";
		return "bg-primary";
	};

	const formatTokens = (n: number) => {
		if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`;
		if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
		return n.toString();
	};

	if (totalTokens === 0) return null;

	if (compact) {
		return (
			<div
				className="w-full h-1 bg-muted overflow-hidden"
				title={`${locale === "de" ? "Kontextfenster" : "Context window"}: ${formatTokens(totalTokens)} / ${formatTokens(maxTokens)} tokens (${percentage.toFixed(0)}%)`}
			>
				<div
					className={cn("h-full transition-all duration-300", getColor())}
					style={{ width: `${percentage}%` }}
				/>
			</div>
		);
	}

	return (
		<div
			className="flex items-center gap-2 text-xs text-muted-foreground"
			title={`${locale === "de" ? "Kontextfenster" : "Context window"}: ${formatTokens(totalTokens)} / ${formatTokens(maxTokens)} tokens`}
		>
			<Gauge className="w-3.5 h-3.5" />
			<div className="flex items-center gap-1.5">
				<div className="w-16 h-1.5 bg-muted rounded-full overflow-hidden">
					<div
						className={cn("h-full transition-all duration-300", getColor())}
						style={{ width: `${percentage}%` }}
					/>
				</div>
				<span className="font-mono text-[10px]">{percentage.toFixed(0)}%</span>
			</div>
		</div>
	);
}
