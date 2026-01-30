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
				"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
				active
					? "bg-primary/15 text-foreground border border-primary"
					: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
			)}
			title={label}
		>
			<Icon className="h-4 w-4" />
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
				<div className="sticky top-0 z-10 bg-card border border-border rounded-t-xl overflow-hidden">
					<div className="flex gap-0.5 p-1 sm:p-2">
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
				<div className="relative flex items-start justify-center gap-3">
					<div className="text-center">
						<h1 className="text-xl font-semibold tracking-tight">{title}</h1>
						<p className="text-sm text-muted-foreground">{subtitle}</p>
					</div>
					<div className="absolute right-0 top-0 flex items-center gap-2 text-xs text-muted-foreground">
						{new Date().toLocaleDateString()}
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
