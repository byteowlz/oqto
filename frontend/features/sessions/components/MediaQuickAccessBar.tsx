"use client";

import { cn } from "@/lib/utils";
import { ImageIcon, Film, Music, LayoutGrid, List, Search, X } from "lucide-react";
import { memo, useState, useRef, useEffect, useCallback } from "react";

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
	/** Search query */
	searchQuery?: string;
	/** Called when search query changes */
	onSearchChange?: (query: string) => void;
}

export const MediaQuickAccessBar = memo(function MediaQuickAccessBar({
	activeFilter,
	onFilterChange,
	imageCount = 0,
	videoCount = 0,
	audioCount = 0,
	searchQuery = "",
	onSearchChange,
}: MediaQuickAccessBarProps) {
	const [searchOpen, setSearchOpen] = useState(false);
	const searchRef = useRef<HTMLInputElement>(null);

	const filters: Array<{ id: MediaType; label: string; icon: typeof ImageIcon; count?: number }> = [
		{ id: "images", label: "Images", icon: ImageIcon, count: imageCount },
		{ id: "videos", label: "Videos", icon: Film, count: videoCount },
		{ id: "audio", label: "Audio", icon: Music, count: audioCount },
	];

	// Focus search input when opened
	useEffect(() => {
		if (searchOpen && searchRef.current) {
			searchRef.current.focus();
		}
	}, [searchOpen]);

	const handleToggleSearch = useCallback(() => {
		if (searchOpen) {
			setSearchOpen(false);
			onSearchChange?.("");
		} else {
			setSearchOpen(true);
		}
	}, [searchOpen, onSearchChange]);

	return (
		<div className="flex items-center gap-1.5 px-2 py-1.5 border-b border-border bg-muted/30 flex-shrink-0">
			{/* Media type filter chips */}
			{filters.map((filter) => {
				const Icon = filter.icon;
				const isActive = activeFilter === filter.id;
				const count = filter.count ?? 0;

				if (count === 0 && !isActive) return null;

				return (
					<button
						key={filter.id}
						type="button"
						onClick={() => onFilterChange(isActive ? "all" : filter.id)}
						className={cn(
							"flex items-center gap-1 px-2 py-1 rounded text-[11px] font-medium transition-colors flex-shrink-0",
							isActive
								? "bg-primary text-primary-foreground"
								: "text-muted-foreground hover:bg-muted hover:text-foreground",
						)}
						title={`${isActive ? "Show all" : `Filter: ${filter.label}`}`}
					>
						<Icon className="w-3 h-3" />
						<span>{count}</span>
					</button>
				);
			})}

			<div className="flex-1" />

			{/* Search */}
			{searchOpen ? (
				<div className="flex items-center gap-1 flex-1 max-w-[200px]">
					<input
						ref={searchRef}
						type="text"
						value={searchQuery}
						onChange={(e) => onSearchChange?.(e.target.value)}
						placeholder="Filter files..."
						className="flex-1 bg-background border border-border rounded px-2 py-0.5 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring min-w-0"
						onKeyDown={(e) => {
							if (e.key === "Escape") {
								handleToggleSearch();
							}
						}}
					/>
					<button
						type="button"
						onClick={handleToggleSearch}
						className="p-1 text-muted-foreground hover:text-foreground rounded"
						title="Close search"
					>
						<X className="w-3 h-3" />
					</button>
				</div>
			) : (
				<button
					type="button"
					onClick={handleToggleSearch}
					className="p-1 text-muted-foreground hover:text-foreground rounded"
					title="Search files (Ctrl+F)"
				>
					<Search className="w-3.5 h-3.5" />
				</button>
			)}
		</div>
	);
});
