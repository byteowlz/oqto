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
				"border border-border bg-muted/30 shadow-none h-full overflow-hidden",
				className,
			)}
		>
			<CardContent className="p-3 xl:p-4 flex items-start gap-2 h-full">
				<div
					className={cn(
						"p-1.5 rounded-md border flex-shrink-0 mt-0.5",
						accent ?? "border-primary/20 text-primary",
					)}
				>
					<Icon className="h-4 w-4" />
				</div>
				<div className="min-w-0 flex-1">
					<p className="text-[10px] uppercase tracking-[0.15em] text-muted-foreground leading-tight">
						{label}
					</p>
					<p className="text-xl xl:text-2xl font-bold mt-0.5 text-foreground font-mono leading-none">
						{value}
					</p>
					{subValue && (
						<p className="text-[11px] text-muted-foreground mt-1 leading-tight">
							{subValue}
						</p>
					)}
				</div>
			</CardContent>
		</Card>
	);
});
