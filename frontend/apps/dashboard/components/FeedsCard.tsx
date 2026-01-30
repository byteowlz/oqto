import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { formatSessionDate } from "@/lib/session-utils";
import { RefreshCw, Rss, Sparkles, X } from "lucide-react";
import { memo, useCallback, useState } from "react";
import type { FeedState } from "../types";

function formatDateTime(value?: string | null): string {
	if (!value) return "";
	const date = new Date(value);
	if (Number.isNaN(date.getTime())) return value;
	return formatSessionDate(date.getTime());
}

export type FeedsCardProps = {
	title: string;
	reloadLabel: string;
	addFeedLabel: string;
	noFeedsLabel: string;
	feedUrls: string[];
	feeds: Record<string, FeedState>;
	onAddFeed: (url: string) => void;
	onRemoveFeed: (url: string) => void;
	onRefreshFeeds: (urls: string[]) => void;
};

export const FeedsCard = memo(function FeedsCard({
	title,
	reloadLabel,
	addFeedLabel,
	noFeedsLabel,
	feedUrls,
	feeds,
	onAddFeed,
	onRemoveFeed,
	onRefreshFeeds,
}: FeedsCardProps) {
	const [feedInput, setFeedInput] = useState("");

	const handleAddFeed = useCallback(() => {
		const trimmed = feedInput.trim();
		if (!trimmed) return;
		onAddFeed(trimmed);
		setFeedInput("");
	}, [feedInput, onAddFeed]);

	return (
		<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
			<CardHeader>
				<CardTitle className="flex items-center gap-2">
					<Rss className="h-4 w-4" />
					{title}
				</CardTitle>
				<CardDescription>RSS / Atom reader</CardDescription>
			</CardHeader>
			<CardContent className="flex-1 min-h-0 overflow-auto space-y-4">
				<div className="flex gap-2">
					<Input
						placeholder="https://example.com/feed.xml"
						value={feedInput}
						onChange={(event) => setFeedInput(event.target.value)}
						onKeyDown={(event) => {
							if (event.key === "Enter") {
								event.preventDefault();
								handleAddFeed();
							}
						}}
					/>
					<Button onClick={handleAddFeed} className="gap-2">
						<Sparkles className="h-4 w-4" />
						{addFeedLabel}
					</Button>
				</div>
				<div className="flex items-center justify-between">
					<p className="text-xs text-muted-foreground">
						{feedUrls.length} feeds tracked
					</p>
					<Button
						variant="ghost"
						size="sm"
						onClick={() => onRefreshFeeds(feedUrls)}
						className="gap-1"
					>
						<RefreshCw className="h-3.5 w-3.5" />
						{reloadLabel}
					</Button>
				</div>

				{feedUrls.length === 0 ? (
					<p className="text-sm text-muted-foreground">{noFeedsLabel}</p>
				) : (
					<div className="space-y-4">
						{feedUrls.map((url) => {
							const feed = feeds[url];
							return (
								<div
									key={url}
									className="border border-border rounded-lg p-3 space-y-2"
								>
									<div className="flex items-start justify-between gap-2">
										<div className="min-w-0">
											<p className="text-sm font-medium truncate">
												{feed?.title || url}
											</p>
											<p className="text-xs text-muted-foreground truncate">
												{url}
											</p>
										</div>
										<Button
											variant="ghost"
											size="icon"
											onClick={() => onRemoveFeed(url)}
										>
											<X className="h-4 w-4" />
										</Button>
									</div>
									{feed?.loading ? (
										<p className="text-xs text-muted-foreground">Loading...</p>
									) : feed?.error ? (
										<p className="text-xs text-rose-400">{feed.error}</p>
									) : (
										<ul className="space-y-1">
											{(feed?.items ?? []).slice(0, 4).map((item) => (
												<li
													key={item.id}
													className="text-xs text-muted-foreground"
												>
													{item.link ? (
														<a
															href={item.link}
															target="_blank"
															rel="noreferrer"
															className="text-foreground hover:underline"
														>
															{item.title}
														</a>
													) : (
														<span className="text-foreground">
															{item.title}
														</span>
													)}
													{item.date && (
														<span className="ml-2">
															{formatDateTime(item.date)}
														</span>
													)}
												</li>
											))}
										</ul>
									)}
								</div>
							);
						})}
					</div>
				)}
			</CardContent>
		</Card>
	);
});
