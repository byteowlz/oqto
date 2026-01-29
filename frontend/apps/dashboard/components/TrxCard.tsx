import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { CheckCircle2, RefreshCw } from "lucide-react";
import { memo } from "react";
import type { TrxIssue } from "../types";

export type TrxCardProps = {
	title: string;
	reloadLabel: string;
	noTrxLabel: string;
	topIssues: TrxIssue[];
	trxStats: {
		total: number;
		open: number;
		inProgress: number;
		blocked: number;
	};
	trxError: string | null;
	trxLoading: boolean;
	onReload: () => void;
};

export const TrxCard = memo(function TrxCard({
	title,
	reloadLabel,
	noTrxLabel,
	topIssues,
	trxStats,
	trxError,
	trxLoading,
	onReload,
}: TrxCardProps) {
	return (
		<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
			<CardHeader className="flex flex-row items-center justify-between">
				<div>
					<CardTitle>{title}</CardTitle>
					<CardDescription>
						{trxError
							? trxError
							: `${trxStats.open} open, ${trxStats.inProgress} in progress, ${trxStats.blocked} blocked`}
					</CardDescription>
				</div>
				<Button
					variant="outline"
					size="sm"
					onClick={onReload}
					disabled={trxLoading}
					className="gap-2"
				>
					<RefreshCw className="h-4 w-4" />
					{reloadLabel}
				</Button>
			</CardHeader>
			<CardContent className="flex-1 min-h-0 overflow-auto space-y-3">
				{topIssues.length === 0 ? (
					<div className="text-sm text-muted-foreground">{noTrxLabel}</div>
				) : (
					<div className="space-y-2">
						{topIssues.map((issue) => (
							<div
								key={issue.id}
								className="flex items-start gap-3 border-b border-border/40 pb-2 last:border-b-0 last:pb-0"
							>
								<CheckCircle2 className="h-4 w-4 text-muted-foreground mt-0.5" />
								<div className="min-w-0">
									<p className="text-sm font-medium truncate">{issue.title}</p>
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
});
