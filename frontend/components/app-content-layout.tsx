"use client";

import { cn } from "@/lib/utils";
import type { LucideIcon } from "lucide-react";
import { type ReactNode, useState } from "react";

export interface SidebarTab {
	id: string;
	label: string;
	icon: LucideIcon;
	content: ReactNode;
	/** Optional badge count */
	badge?: number;
}

interface AppContentLayoutProps {
	/** Main content (shown as first tab on mobile) */
	children: ReactNode;
	/** Label for main content tab on mobile */
	mainTabLabel: string;
	/** Icon for main content tab on mobile */
	mainTabIcon: LucideIcon;
	/** Sidebar tabs (right panel on desktop, additional tabs on mobile) */
	sidebarTabs?: SidebarTab[];
	/** Optional header content shown above main content */
	header?: ReactNode;
	/** Default active sidebar tab id */
	defaultSidebarTab?: string;
	/** Class name for the main content area */
	mainClassName?: string;
	/** Class name for the sidebar area */
	sidebarClassName?: string;
}

interface TabButtonProps {
	active: boolean;
	onClick: () => void;
	icon: LucideIcon;
	label: string;
	badge?: number;
	hideLabel?: boolean;
}

function TabButton({
	active,
	onClick,
	icon: Icon,
	label,
	badge,
	hideLabel,
}: TabButtonProps) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={cn(
				"flex items-center justify-center gap-1.5 px-2 sm:px-3 py-1.5 text-xs sm:text-sm font-medium transition-colors flex-1 sm:flex-initial min-w-0",
				active
					? "bg-primary text-primary-foreground"
					: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
			)}
		>
			<Icon className="w-4 h-4 flex-shrink-0" />
			{!hideLabel && <span className="truncate">{label}</span>}
			{badge !== undefined && badge > 0 && (
				<span
					className={cn(
						"ml-1 px-1.5 py-0.5 text-[10px] font-medium rounded-full flex-shrink-0",
						active ? "bg-primary-foreground/20" : "bg-primary/20 text-primary",
					)}
				>
					{badge}
				</span>
			)}
		</button>
	);
}

export function AppContentLayout({
	children,
	mainTabLabel,
	mainTabIcon,
	sidebarTabs = [],
	header,
	defaultSidebarTab,
	mainClassName,
	sidebarClassName,
}: AppContentLayoutProps) {
	// On mobile, "main" is a tab. On desktop, sidebar tabs only.
	const [activeTab, setActiveTab] = useState<string>(
		defaultSidebarTab || sidebarTabs[0]?.id || "main",
	);

	const hasSidebar = sidebarTabs.length > 0;

	// Find active sidebar content
	const activeSidebarContent = sidebarTabs.find(
		(tab) => tab.id === activeTab,
	)?.content;

	return (
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4">
			{/* Mobile layout: single panel with tabs */}
			<div className="flex-1 min-h-0 flex flex-col lg:hidden">
				{/* Mobile tabs - sticky at top */}
				{hasSidebar && (
					<div className="sticky top-0 z-10 bg-card border border-border rounded-t-xl overflow-hidden">
						<div className="flex gap-0.5 p-1 sm:p-2">
							<TabButton
								active={activeTab === "main"}
								onClick={() => setActiveTab("main")}
								icon={mainTabIcon}
								label={mainTabLabel}
							/>
							{sidebarTabs.map((tab) => (
								<TabButton
									key={tab.id}
									active={activeTab === tab.id}
									onClick={() => setActiveTab(tab.id)}
									icon={tab.icon}
									label={tab.label}
									badge={tab.badge}
								/>
							))}
						</div>
					</div>
				)}

				{/* Mobile content */}
				<div
					className={cn(
						"flex-1 min-h-0 bg-card border border-border p-1.5 sm:p-4 overflow-hidden flex flex-col",
						hasSidebar ? "border-t-0 rounded-b-xl" : "rounded-xl",
					)}
				>
					{activeTab === "main" ? (
						<>
							{header}
							{children}
						</>
					) : (
						activeSidebarContent
					)}
				</div>
			</div>

			{/* Desktop layout: side by side */}
			<div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
				{/* Main panel */}
				<div
					className={cn(
						"flex-[3] min-w-0 bg-card border border-border p-4 xl:p-6 flex flex-col min-h-0 h-full",
						mainClassName,
					)}
				>
					{header}
					{children}
				</div>

				{/* Sidebar panel */}
				{hasSidebar && (
					<div
						className={cn(
							"flex-[2] min-w-[320px] max-w-[420px] bg-card border border-border flex flex-col min-h-0 h-full",
							sidebarClassName,
						)}
					>
						<div className="flex gap-1 p-2 border-b border-border">
							{sidebarTabs.map((tab) => (
								<TabButton
									key={tab.id}
									active={activeTab === tab.id}
									onClick={() => setActiveTab(tab.id)}
									icon={tab.icon}
									label={tab.label}
									badge={tab.badge}
									hideLabel
								/>
							))}
						</div>
						<div className="flex-1 min-h-0 overflow-hidden">
							{activeSidebarContent}
						</div>
					</div>
				)}
			</div>
		</div>
	);
}
