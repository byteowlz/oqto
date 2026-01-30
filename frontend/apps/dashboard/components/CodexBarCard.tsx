import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { formatSessionDate } from "@/lib/session-utils";
import { RefreshCw } from "lucide-react";
import { memo } from "react";
import type { CodexBarState } from "../types";

function formatDateTime(value?: string | null): string {
	if (!value) return "";
	const date = new Date(value);
	if (Number.isNaN(date.getTime())) return value;
	return formatSessionDate(date.getTime());
}

export type CodexBarCardProps = {
	reloadLabel: string;
	codexbar: CodexBarState;
	onReload: () => void;
};

export const CodexBarCard = memo(function CodexBarCard({
	reloadLabel,
	codexbar,
	onReload,
}: CodexBarCardProps) {
	const codexbarEntries = codexbar.payload ?? [];

	return (
		<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
			<CardHeader className="flex flex-row items-center justify-between">
				<div>
					<CardTitle>AI Subscriptions</CardTitle>
					<CardDescription>CodexBar usage snapshots</CardDescription>
				</div>
				<Button
					variant="outline"
					size="sm"
					onClick={onReload}
					disabled={codexbar.loading}
					className="gap-2"
				>
					<RefreshCw className="h-4 w-4" />
					{reloadLabel}
				</Button>
			</CardHeader>
			<CardContent className="flex-1 min-h-0 overflow-auto space-y-3">
				{!codexbar.available ? (
					<p className="text-sm text-muted-foreground">
						CodexBar is not available on this host.
					</p>
				) : (
					<>
						{codexbar.error && (
							<p className="text-xs text-rose-400">{codexbar.error}</p>
						)}
						{codexbarEntries.length === 0 ? (
							<p className="text-sm text-muted-foreground">
								No CodexBar data yet.
							</p>
						) : (
							<div className="space-y-3">
								{codexbarEntries.slice(0, 6).map((entry) => {
									const primary = entry.usage?.primary;
									const secondary = entry.usage?.secondary;
									const credits = entry.credits?.remaining;
									return (
										<div
											key={`${entry.provider}-${entry.account ?? ""}`}
											className="border-b border-border/40 pb-3 last:border-b-0 last:pb-0"
										>
											<div className="flex items-center justify-between gap-2">
												<div className="min-w-0">
													<p className="text-sm font-medium truncate">
														{entry.provider}
													</p>
													<p className="text-xs text-muted-foreground truncate">
														{entry.account ??
															entry.usage?.accountEmail ??
															entry.source}
													</p>
												</div>
												{entry.status?.indicator && (
													<Badge variant="secondary">
														{entry.status.indicator}
													</Badge>
												)}
											</div>
											<div className="mt-2 grid grid-cols-2 gap-2 text-xs text-muted-foreground">
												<div>
													Session:{" "}
													{primary?.usedPercent != null
														? `${primary.usedPercent}% used`
														: "n/a"}
													{primary?.resetsAt && (
														<span className="block">
															Resets {formatDateTime(primary.resetsAt)}
														</span>
													)}
												</div>
												<div>
													Weekly:{" "}
													{secondary?.usedPercent != null
														? `${secondary.usedPercent}% used`
														: "n/a"}
													{secondary?.resetsAt && (
														<span className="block">
															Resets {formatDateTime(secondary.resetsAt)}
														</span>
													)}
												</div>
											</div>
											{credits != null && (
												<p className="text-xs text-muted-foreground mt-2">
													Credits: {credits}
												</p>
											)}
										</div>
									);
								})}
							</div>
						)}
					</>
				)}
			</CardContent>
		</Card>
	);
});
