"use client";

import { cn } from "@/lib/utils";
import { FolderOpen, MessageSquareText } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AgentFilter, SearchMode } from "./SearchResults";

interface SearchModeToggleProps {
	mode: SearchMode;
	onModeChange: (mode: SearchMode) => void;
	agentFilter: AgentFilter;
	onAgentFilterChange: (filter: AgentFilter) => void;
	locale: "en" | "de";
	className?: string;
}

export function SearchModeToggle({
	mode,
	onModeChange,
	agentFilter,
	onAgentFilterChange,
	locale,
	className,
}: SearchModeToggleProps) {
	const { t } = useTranslation();
	return (
		<div className={cn("flex items-center gap-1.5 flex-wrap", className)}>
			{/* Mode toggle */}
			<div className="flex items-center bg-sidebar-accent rounded p-0.5 border border-sidebar-border">
				<button
					type="button"
					onClick={() => onModeChange("sessions")}
					className={cn(
						"flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded transition-colors",
						mode === "sessions"
							? "bg-primary text-primary-foreground shadow-sm"
							: "text-muted-foreground hover:text-foreground hover:bg-sidebar-accent",
					)}
					title="Ctrl+Shift+F"
				>
					<FolderOpen className="w-3.5 h-3.5" />
					{t("search.sessions")}
				</button>
				<button
					type="button"
					onClick={() => onModeChange("messages")}
					className={cn(
						"flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded transition-colors",
						mode === "messages"
							? "bg-primary text-primary-foreground shadow-sm"
							: "text-muted-foreground hover:text-foreground hover:bg-sidebar-accent",
					)}
					title="Ctrl+Shift+F"
				>
					<MessageSquareText className="w-3.5 h-3.5" />
					{t("search.messages")}
				</button>
			</div>

			{/* Agent filter (only visible in messages mode) */}
			{mode === "messages" && (
				<div className="flex items-center bg-sidebar-accent rounded p-0.5 border border-sidebar-border">
					<button
						type="button"
						onClick={() => onAgentFilterChange("all")}
						className={cn(
							"px-2 py-1 text-xs font-medium rounded transition-colors",
							agentFilter === "all"
								? "bg-background text-foreground shadow-sm"
								: "text-muted-foreground hover:text-foreground hover:bg-sidebar-accent",
						)}
					>
						{t("search.all")}
					</button>
					<button
						type="button"
						onClick={() => onAgentFilterChange("pi_agent")}
						className={cn(
							"px-2 py-1 text-xs font-medium rounded transition-colors",
							agentFilter === "pi_agent"
								? "bg-purple-500/30 text-purple-400 shadow-sm"
								: "text-muted-foreground hover:text-foreground hover:bg-sidebar-accent",
						)}
					>
						{t("search.defaultChat")}
					</button>
				</div>
			)}
		</div>
	);
}
