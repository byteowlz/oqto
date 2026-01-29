import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { RefreshCw } from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";

export type QueryCardProps = {
	title: string;
	description?: string;
	url?: string;
	method?: string;
	headers?: Record<string, string>;
};

export const QueryCard = memo(function QueryCard({
	title,
	description,
	url,
	method,
	headers,
}: QueryCardProps) {
	const [data, setData] = useState<unknown>(null);
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);

	const handleLoad = useCallback(async () => {
		if (!url) return;
		setLoading(true);
		setError(null);
		try {
			const res = await fetch(url, {
				method: method ?? "GET",
				headers,
				credentials: "include",
			});
			const contentType = res.headers.get("content-type") ?? "";
			if (!res.ok) {
				const text = await res.text();
				throw new Error(text || "Query failed");
			}
			if (contentType.includes("application/json")) {
				setData(await res.json());
			} else {
				setData(await res.text());
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : "Query failed");
		} finally {
			setLoading(false);
		}
	}, [headers, method, url]);

	useEffect(() => {
		if (url) {
			handleLoad();
		}
	}, [handleLoad, url]);

	return (
		<Card className="border-border bg-muted/30 shadow-none h-full flex flex-col">
			<CardHeader className="flex flex-row items-center justify-between">
				<div>
					<CardTitle>{title}</CardTitle>
					{description && <CardDescription>{description}</CardDescription>}
				</div>
				<Button
					variant="outline"
					size="sm"
					onClick={handleLoad}
					disabled={loading || !url}
					className="gap-2"
				>
					<RefreshCw className="h-4 w-4" />
					Refresh
				</Button>
			</CardHeader>
			<CardContent className="flex-1 overflow-auto">
				{!url ? (
					<p className="text-sm text-muted-foreground">No URL configured.</p>
				) : loading ? (
					<p className="text-sm text-muted-foreground">Loading...</p>
				) : error ? (
					<p className="text-sm text-rose-400">{error}</p>
				) : (
					<pre className="text-xs whitespace-pre-wrap text-muted-foreground">
						{typeof data === "string" ? data : JSON.stringify(data, null, 2)}
					</pre>
				)}
			</CardContent>
		</Card>
	);
});
