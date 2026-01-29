import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { memo } from "react";

export type StatCardProps = {
	label: string;
	value: string | number;
	subValue?: string;
	Icon: React.ElementType;
	accent?: string;
	className?: string;
};

export const StatCard = memo(function StatCard({
	label,
	value,
	subValue,
	Icon,
	accent,
	className,
}: StatCardProps) {
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
