"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardFooter,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
import {
	listFlavors,
	listSkeletons,
	listSlides,
	previewSkeleton,
	type Flavor,
	type Skeleton,
	type SlideSummary,
} from "@/lib/sldr-client";
import {
	ExternalLink,
	FileText,
	LayoutTemplate,
	Palette,
	RefreshCw,
	Search,
	Sparkles,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

const EMPTY_STATE = "No items yet";

export function SldrScreen() {
	const [slides, setSlides] = useState<SlideSummary[]>([]);
	const [skeletons, setSkeletons] = useState<Skeleton[]>([]);
	const [flavors, setFlavors] = useState<Flavor[]>([]);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [query, setQuery] = useState("");
	const [previewing, setPreviewing] = useState<Record<string, boolean>>({});
	const [previewUrls, setPreviewUrls] = useState<Record<string, string>>({});

	const refresh = useCallback(async () => {
		setLoading(true);
		setError(null);
		try {
			const [slidesData, skeletonsData, flavorsData] = await Promise.all([
				listSlides(),
				listSkeletons(),
				listFlavors(),
			]);
			setSlides(slidesData);
			setSkeletons(skeletonsData);
			setFlavors(flavorsData);
		} catch (err) {
			const message = err instanceof Error ? err.message : "Failed to load sldr";
			setError(message);
		} finally {
			setLoading(false);
		}
	}, []);

	useEffect(() => {
		void refresh();
	}, [refresh]);

	const filteredSlides = useMemo(() => {
		if (!query.trim()) return slides;
		const needle = query.toLowerCase();
		return slides.filter((slide) => {
			const title = slide.metadata.title?.toLowerCase() ?? "";
			const tags = slide.metadata.tags?.join(" ").toLowerCase() ?? "";
			return (
				slide.name.toLowerCase().includes(needle) ||
				slide.relative_path.toLowerCase().includes(needle) ||
				title.includes(needle) ||
				tags.includes(needle)
			);
		});
	}, [query, slides]);

	const slideCount = slides.length;
	const skeletonCount = skeletons.length;
	const flavorCount = flavors.length;

	const handlePreview = useCallback(async (name: string, flavor?: string) => {
		setPreviewing((prev) => ({ ...prev, [name]: true }));
		setError(null);
		try {
			const result = await previewSkeleton(name, flavor);
			setPreviewUrls((prev) => ({ ...prev, [name]: result.url }));
			window.open(result.url, "_blank", "noopener,noreferrer");
		} catch (err) {
			const message = err instanceof Error ? err.message : "Preview failed";
			setError(message);
		} finally {
			setPreviewing((prev) => ({ ...prev, [name]: false }));
		}
	}, []);

	return (
		<div className="flex h-full flex-col gap-6 p-6">
			<div className="flex flex-wrap items-center justify-between gap-4">
				<div>
					<p className="text-muted-foreground text-xs uppercase tracking-[0.2em]">
						Presentation Studio
					</p>
					<h1 className="text-2xl font-semibold">sldr</h1>
					<p className="text-muted-foreground mt-1 max-w-xl text-sm">
						Compose slide decks from reusable markdown, manage skeletons, and
						swap flavors without leaving Octo.
					</p>
				</div>
				<div className="flex items-center gap-2">
					<Button
						variant="outline"
						size="sm"
						onClick={refresh}
						disabled={loading}
					>
						<RefreshCw className={cn("mr-2 size-4", loading && "animate-spin")} />
						Refresh
					</Button>
				</div>
			</div>

			{error ? (
				<div className="text-sm text-red-500">{error}</div>
			) : null}

			<div className="grid gap-4 lg:grid-cols-[1.4fr_0.6fr]">
				<Card className="relative overflow-hidden">
					<CardHeader>
						<CardTitle className="flex items-center gap-2">
							<Sparkles className="size-4 text-emerald-500" />
							Quick actions
						</CardTitle>
						<CardDescription>
							Open a skeleton preview or jump into layout editing.
						</CardDescription>
					</CardHeader>
					<CardContent className="flex flex-col gap-3">
						<div className="flex flex-wrap items-center gap-3">
							<div className="relative flex-1">
								<Search className="text-muted-foreground absolute left-3 top-2.5 size-4" />
								<Input
									value={query}
									onChange={(event) => setQuery(event.target.value)}
									placeholder="Search slides, tags, or paths"
									className="pl-9"
								/>
							</div>
							<Button
								variant="secondary"
								size="sm"
								onClick={() => setQuery("")}
								disabled={!query}
							>
								Clear
							</Button>
						</div>
						<p className="text-muted-foreground text-xs">
							Preview a skeleton by clicking the play icon in the skeleton list.
						</p>
					</CardContent>
				</Card>

				<Card>
					<CardHeader>
						<CardTitle>Library status</CardTitle>
						<CardDescription>
							Current inventory across slides, skeletons, and flavors.
						</CardDescription>
					</CardHeader>
					<CardContent className="grid gap-3">
						<div className="flex items-center justify-between rounded-lg border px-4 py-3">
							<div className="flex items-center gap-2 text-sm">
								<FileText className="text-muted-foreground size-4" />
								Slides
							</div>
							<span className="text-sm font-semibold">{slideCount}</span>
						</div>
						<div className="flex items-center justify-between rounded-lg border px-4 py-3">
							<div className="flex items-center gap-2 text-sm">
								<LayoutTemplate className="text-muted-foreground size-4" />
								Skeletons
							</div>
							<span className="text-sm font-semibold">{skeletonCount}</span>
						</div>
						<div className="flex items-center justify-between rounded-lg border px-4 py-3">
							<div className="flex items-center gap-2 text-sm">
								<Palette className="text-muted-foreground size-4" />
								Flavors
							</div>
							<span className="text-sm font-semibold">{flavorCount}</span>
						</div>
					</CardContent>
				</Card>
			</div>

			<Tabs defaultValue="slides" className="flex-1">
				<TabsList>
					<TabsTrigger value="slides">Slides</TabsTrigger>
					<TabsTrigger value="skeletons">Skeletons</TabsTrigger>
					<TabsTrigger value="flavors">Flavors</TabsTrigger>
				</TabsList>

				<TabsContent value="slides" className="flex-1">
					<ScrollArea className="h-[520px] pr-4">
						<div className="grid gap-4 lg:grid-cols-2">
							{filteredSlides.length === 0 ? (
								<div className="text-muted-foreground text-sm">
									{loading ? "Loading slides..." : EMPTY_STATE}
								</div>
							) : (
								filteredSlides.map((slide) => (
									<Card key={slide.relative_path}>
										<CardHeader>
											<CardTitle>
												{slide.metadata.title || slide.name}
											</CardTitle>
											<CardDescription>
												{slide.relative_path}
											</CardDescription>
										</CardHeader>
										<CardContent className="flex flex-wrap gap-2">
											{slide.metadata.topic ? (
												<Badge variant="secondary">{slide.metadata.topic}</Badge>
											) : null}
											{slide.metadata.tags?.map((tag) => (
												<Badge key={tag} variant="outline">
													{tag}
												</Badge>
											))}
											{slide.metadata.layout ? (
												<Badge variant="outline">{slide.metadata.layout}</Badge>
											) : null}
										</CardContent>
										<CardFooter className="text-muted-foreground text-xs">
											{slide.metadata.description || "No description"}
										</CardFooter>
									</Card>
								))
							)}
						</div>
					</ScrollArea>
				</TabsContent>

				<TabsContent value="skeletons" className="flex-1">
					<ScrollArea className="h-[520px] pr-4">
						<div className="grid gap-4 lg:grid-cols-2">
							{skeletons.length === 0 ? (
								<div className="text-muted-foreground text-sm">
									{loading ? "Loading skeletons..." : EMPTY_STATE}
								</div>
							) : (
								skeletons.map((skeleton) => (
									<Card key={skeleton.name}>
										<CardHeader>
											<CardTitle>
												{skeleton.title || skeleton.name}
											</CardTitle>
											<CardDescription>
												{skeleton.description ||
													`${skeleton.slides.length} slides`}
											</CardDescription>
										</CardHeader>
										<CardContent className="flex flex-wrap gap-2">
											<Badge variant="secondary">
												{skeleton.slides.length} slides
											</Badge>
											{skeleton.flavor ? (
												<Badge variant="outline">{skeleton.flavor}</Badge>
											) : null}
										</CardContent>
										<CardFooter className="flex items-center justify-between gap-2">
											<Button
												variant="secondary"
												size="sm"
												onClick={() =>
													handlePreview(skeleton.name, skeleton.flavor)
												}
												disabled={previewing[skeleton.name]}
											>
												<ExternalLink className="mr-2 size-4" />
												{previewing[skeleton.name] ? "Launching" : "Preview"}
											</Button>
											{previewUrls[skeleton.name] ? (
												<Button
													variant="ghost"
													size="sm"
													onClick={() =>
														window.open(
															previewUrls[skeleton.name],
															"_blank",
															"noopener,noreferrer",
														)
													}
												>
													Open
												</Button>
											) : null}
										</CardFooter>
									</Card>
								))
							)}
						</div>
					</ScrollArea>
				</TabsContent>

				<TabsContent value="flavors" className="flex-1">
					<ScrollArea className="h-[520px] pr-4">
						<div className="grid gap-4 lg:grid-cols-2">
							{flavors.length === 0 ? (
								<div className="text-muted-foreground text-sm">
									{loading ? "Loading flavors..." : EMPTY_STATE}
								</div>
							) : (
								flavors.map((flavor) => (
									<Card key={flavor.name}>
										<CardHeader>
											<CardTitle>
												{flavor.display_name || flavor.name}
											</CardTitle>
											<CardDescription>
												{flavor.description || "No description"}
											</CardDescription>
										</CardHeader>
										<CardContent className="grid gap-3">
											<div className="flex items-center gap-2">
												{renderSwatch(flavor.colors?.primary)}
												{renderSwatch(flavor.colors?.secondary)}
												{renderSwatch(flavor.colors?.background, true)}
											</div>
											<div className="text-muted-foreground text-xs">
												{flavor.typography?.heading_font || "Default fonts"}
											</div>
										</CardContent>
									</Card>
								))
							)}
						</div>
					</ScrollArea>
				</TabsContent>
			</Tabs>
		</div>
	);
}

function renderSwatch(color?: string, outline = false) {
	const fallback = "var(--muted)";
	return (
		<span
			className={cn(
				"inline-flex h-6 w-6 items-center justify-center rounded-full",
				outline && "border",
			)}
			style={{ backgroundColor: color ?? fallback }}
			title={color}
		/>
	);
}
