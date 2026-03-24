"use client";

import { cn } from "@/lib/utils";
import { ImageIcon, Film, Music, LayoutGrid, List, ChevronDown } from "lucide-react";
import { memo, useState } from "react";

export type MediaType = "all" | "images" | "videos" | "audio";

export interface MediaQuickAccessBarProps {
	/** Current active media filter */
	activeFilter: MediaType;
	/** Called when filter changes */
	onFilterChange: (filter: MediaType) => void;
	/** Count of images in current view */
	imageCount?: number;
	/** Count of videos in current view */
	videoCount?: number;
	/** Count of audio files in current view */
	audioCount?: number;
	/** Show view mode toggle */
	showViewModeToggle?: boolean;
	/** Current view mode */
	viewMode?: "grid" | "list" | "tree";
	/** Called when view mode changes */
 onViewModeChange?: (mode: "grid" | "list" | "tree") => void;
}

export const MediaQuickAccessBar = memo(function MediaQuickAccessBar({
	activeFilter,
	onFilterChange,
	imageCount = 0,
	videoCount = 0,
	audioCount = 0,
	showViewModeToggle = false,
	viewMode = "grid",
	onViewModeChange,
}: MediaQuickAccessBarProps) {
	const [showDropdown, setShowDropdown] = useState(false);

	const filters = [
		{ id: "all" as MediaType, label: "All Files", icon: LayoutGrid },
		{ id: "images" as MediaType, label: "Images", icon: ImageIcon, count: imageCount },
		{ id: "videos" as MediaType, label: "Videos", icon: Film, count: videoCount },
		{ id: "audio" as MediaType, label: "Audio", icon: Music, count: audioCount },
	];

	const activeFilterConfig = filters.find((f) => f.id === activeFilter);

	return (
		<div className="flex items-center gap-2 px-3 py-2 border-b border-border bg-muted/30">
			{/* Media filters */}
			<div className="flex items-center gap-1 flex-shrink-0">
				{filters.map((filter) => {
					const Icon = filter.icon;
					const isActive = activeFilter === filter.id;

					return (
						<button
							key={filter.id}
							type="button"
							onClick={() => onFilterChange(filter.id)}
							className={cn(
								"flex items-center gap-2 px-3 py-1.5 rounded-md text-sm font-medium transition-colors",
								isActive
									? "bg-primary text-primary-foreground"
									: "text-muted-foreground hover:bg-muted hover:text-foreground",
							)}
							title={`Filter by ${filter.label.toLowerCase()}`}
						>
							<Icon className="w-4 h-4" />
							<span className="whitespace-nowrap">{filter.label}</span>
							{filter.count !== undefined && (
								<span
									className={cn(
										"text-xs px-1.5 py-0.5 rounded",
										isActive
											? "bg-primary-foreground/20 text-primary-foreground"
											: "bg-muted text-muted-foreground",
									)}
								>
									{filter.count}
								</span>
							)}
						</button>
					);
				})}
			</div>

			{/* View mode toggle */}
			{showViewModeToggle && onViewModeChange && (
				<div className="ml-auto flex items-center gap-1 border-l border-border pl-3">
					<button
						type="button"
						onClick={() => onViewModeChange("tree")}
						className={cn(
							"p-1.5 rounded-md transition-colors",
							viewMode === "tree"
								? "bg-primary/20 text-primary"
								: "text-muted-foreground hover:bg-muted hover:text-foreground",
						)}
						title="Tree view"
					>
						<LayoutGrid className="w-4 h-4" />
					</button>
					<button
						type="button"
						onClick={() => onViewModeChange("list")}
						className={cn(
							"p-1.5 rounded-md transition-colors",
							viewMode === "list"
								? "bg-primary/20 text-primary"
								: "text-muted-foreground hover:bg-muted hover:text-foreground",
						)}
						title="List view"
					>
						<List className="w-4 h-4" />
					</button>
				</div>
			)}
		</div>
	);
});
