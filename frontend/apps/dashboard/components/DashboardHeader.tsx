import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
	Activity,
	GripVertical,
	ListTodo,
	PanelLeftClose,
	PanelRightClose,
	Sparkles,
} from "lucide-react";
import { memo } from "react";

type MobileView = "dashboard" | "cards" | "custom";
type SidebarSection = "cards" | "custom";

function MobileTabButton({
	active,
	label,
	icon: Icon,
	onClick,
}: {
	active: boolean;
	label: string;
	icon: React.ElementType;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={cn(
				"flex-1 flex flex-col items-center justify-center gap-1 px-2 py-2 rounded-lg text-[11px] uppercase tracking-wide transition-colors",
				active
					? "bg-primary/15 text-foreground border border-primary/50"
					: "text-muted-foreground border border-border/40 bg-muted/40 hover:border-border hover:bg-muted/60",
			)}
			title={label}
		>
			<Icon className="h-4 w-4" />
			<span>{label}</span>
		</button>
	);
}

export type DashboardHeaderProps = {
	title: string;
	subtitle: string;
	customCardsLabel: string;
	layoutEditMode: boolean;
	setLayoutEditMode: (value: boolean | ((prev: boolean) => boolean)) => void;
	// Desktop only
	rightSidebarCollapsed?: boolean;
	setRightSidebarCollapsed?: (
		value: boolean | ((prev: boolean) => boolean),
	) => void;
	// Mobile only
	mobileView?: MobileView;
	setMobileView?: (view: MobileView) => void;
	setSidebarSection?: (section: SidebarSection) => void;
	isMobile: boolean;
	layoutError?: string | null;
};

export const DashboardHeader = memo(function DashboardHeader({
	title,
	subtitle,
	customCardsLabel,
	layoutEditMode,
	setLayoutEditMode,
	rightSidebarCollapsed,
	setRightSidebarCollapsed,
	mobileView,
	setMobileView,
	setSidebarSection,
	isMobile,
	layoutError,
}: DashboardHeaderProps) {
	if (isMobile) {
		return (
			<>
				<div className="sticky top-0 z-10 bg-card/95 backdrop-blur border border-border/60 rounded-t-2xl overflow-hidden shadow-sm">
					<div className="grid grid-cols-3 gap-1.5 p-2">
						<MobileTabButton
							active={mobileView === "dashboard"}
							icon={Activity}
							label={title}
							onClick={() => setMobileView?.("dashboard")}
						/>
						<MobileTabButton
							active={mobileView === "cards"}
							icon={ListTodo}
							label="Cards"
							onClick={() => {
								setMobileView?.("cards");
								setSidebarSection?.("cards");
							}}
						/>
						<MobileTabButton
							active={mobileView === "custom"}
							icon={Sparkles}
							label={customCardsLabel}
							onClick={() => {
								setMobileView?.("custom");
								setSidebarSection?.("custom");
							}}
						/>
					</div>
				</div>
				<div className="flex items-center justify-between gap-3 px-1.5 pt-3">
					<div className="min-w-0">
						<p className="text-[10px] uppercase tracking-[0.25em] text-muted-foreground truncate">
							{subtitle}
						</p>
						<h1 className="text-lg font-semibold tracking-tight truncate">
							{title}
						</h1>
					</div>
					<div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
						<span className="whitespace-nowrap">
							{new Date().toLocaleDateString()}
						</span>
						<Button
							variant={layoutEditMode ? "secondary" : "ghost"}
							size="icon"
							className="size-7"
							onClick={() => setLayoutEditMode((prev) => !prev)}
						>
							<GripVertical className="size-4" />
						</Button>
					</div>
				</div>
				{layoutError && (
					<div className="text-sm text-rose-400">{layoutError}</div>
				)}
			</>
		);
	}

	return (
		<>
			<div className="flex items-start justify-between gap-3">
				<div>
					<h1 className="text-2xl md:text-3xl font-semibold tracking-tight">
						{title}
					</h1>
					<p className="text-sm text-muted-foreground">{subtitle}</p>
				</div>
				<div className="flex items-center gap-2 text-xs text-muted-foreground">
					{new Date().toLocaleDateString()}
					<Button
						variant={layoutEditMode ? "secondary" : "ghost"}
						size="icon"
						className="size-7"
						onClick={() => setLayoutEditMode((prev) => !prev)}
					>
						<GripVertical className="size-4" />
					</Button>
					{setRightSidebarCollapsed && (
						<button
							type="button"
							onClick={() => setRightSidebarCollapsed((prev) => !prev)}
							className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
							title={
								rightSidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"
							}
						>
							{rightSidebarCollapsed ? (
								<PanelLeftClose className="size-4" />
							) : (
								<PanelRightClose className="size-4" />
							)}
						</button>
					)}
				</div>
			</div>
			{layoutError && (
				<div className="text-sm text-rose-400 mt-2">{layoutError}</div>
			)}
		</>
	);
});
