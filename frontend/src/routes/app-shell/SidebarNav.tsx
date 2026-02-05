import { Button } from "@/components/ui/button";
import {
	Globe2,
	LayoutDashboard,
	LogOut,
	MoonStar,
	Settings,
	Shield,
	SunMedium,
} from "lucide-react";
import { memo } from "react";

export interface SidebarNavProps {
	activeAppId: string;
	sidebarCollapsed: boolean;
	isDark: boolean;
	isAdmin?: boolean;
	onToggleApp: (appId: string) => void;
	onToggleLocale: () => void;
	onToggleTheme: () => void;
	onLogout: () => void;
}

// Style constants
const navIdle = "var(--sidebar, #181b1a)";
const navText = "var(--sidebar-foreground, #dfe5e1)";
const navActiveBg = "#3ba77c";
const navActiveText = "#0b0f0d";
const navActiveBorder = "#3ba77c";
const sidebarHover = "rgba(59, 167, 124, 0.12)";
const sidebarHoverBorder = "transparent";

export const SidebarNav = memo(function SidebarNav({
	activeAppId,
	sidebarCollapsed,
	isDark,
	isAdmin,
	onToggleApp,
	onToggleLocale,
	onToggleTheme,
	onLogout,
}: SidebarNavProps) {
	return (
		<div
			className={`w-full ${sidebarCollapsed ? "px-2 pb-3" : "px-5 pb-4"} mt-auto pt-3`}
		>
			<div className="h-px w-full bg-primary/50 mb-3" />
			<div
				className={`flex items-center ${sidebarCollapsed ? "flex-col gap-2" : `justify-center ${isAdmin ? "gap-1" : "gap-2"}`}`}
			>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					rounded="full"
					onClick={() => onToggleApp("dashboard")}
					aria-label="Dashboard"
					className="w-9 h-9 flex items-center justify-center transition-colors"
					style={{
						backgroundColor:
							activeAppId === "dashboard" ? navActiveBg : navIdle,
						border:
							activeAppId === "dashboard"
								? `1px solid ${navActiveBorder}`
								: "1px solid transparent",
						color: activeAppId === "dashboard" ? navActiveText : navText,
					}}
					onMouseEnter={(e) => {
						if (activeAppId !== "dashboard") {
							e.currentTarget.style.backgroundColor = sidebarHover;
							e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
						}
					}}
					onMouseLeave={(e) => {
						if (activeAppId !== "dashboard") {
							e.currentTarget.style.backgroundColor = navIdle;
							e.currentTarget.style.border = "1px solid transparent";
						}
					}}
				>
					<LayoutDashboard className="w-4 h-4" />
				</Button>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					rounded="full"
					onClick={() => onToggleApp("settings")}
					aria-label="Settings"
					className="w-9 h-9 flex items-center justify-center transition-colors"
					style={{
						backgroundColor: activeAppId === "settings" ? navActiveBg : navIdle,
						border:
							activeAppId === "settings"
								? `1px solid ${navActiveBorder}`
								: "1px solid transparent",
						color: activeAppId === "settings" ? navActiveText : navText,
					}}
					onMouseEnter={(e) => {
						if (activeAppId !== "settings") {
							e.currentTarget.style.backgroundColor = sidebarHover;
							e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
						}
					}}
					onMouseLeave={(e) => {
						if (activeAppId !== "settings") {
							e.currentTarget.style.backgroundColor = navIdle;
							e.currentTarget.style.border = "1px solid transparent";
						}
					}}
				>
					<Settings className="w-4 h-4" />
				</Button>
			{isAdmin && (
				<Button
					type="button"
					variant="ghost"
					size="icon"
					rounded="full"
					onClick={() => onToggleApp("admin")}
					aria-label="Admin"
					className="w-9 h-9 flex items-center justify-center transition-colors"
					style={{
						backgroundColor: activeAppId === "admin" ? navActiveBg : navIdle,
						border:
							activeAppId === "admin"
								? `1px solid ${navActiveBorder}`
								: "1px solid transparent",
						color: activeAppId === "admin" ? navActiveText : navText,
					}}
					onMouseEnter={(e) => {
						if (activeAppId !== "admin") {
							e.currentTarget.style.backgroundColor = sidebarHover;
							e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
						}
					}}
					onMouseLeave={(e) => {
						if (activeAppId !== "admin") {
							e.currentTarget.style.backgroundColor = navIdle;
							e.currentTarget.style.border = "1px solid transparent";
						}
					}}
				>
					<Shield className="w-4 h-4" />
				</Button>
			)}
				<Button
					type="button"
					variant="ghost"
					size="icon"
					rounded="full"
					onClick={onToggleLocale}
					aria-label="Sprache wechseln"
					className="w-9 h-9 flex items-center justify-center transition-colors"
					style={{
						backgroundColor: navIdle,
						border: "1px solid transparent",
						color: navText,
					}}
					onMouseEnter={(e) => {
						e.currentTarget.style.backgroundColor = sidebarHover;
						e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
					}}
					onMouseLeave={(e) => {
						e.currentTarget.style.backgroundColor = navIdle;
						e.currentTarget.style.border = "1px solid transparent";
					}}
				>
					<Globe2 className="w-4 h-4" />
				</Button>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					rounded="full"
					onClick={onToggleTheme}
					aria-pressed={isDark}
					className="w-9 h-9 flex items-center justify-center transition-colors"
					style={{
						backgroundColor: navIdle,
						border: "1px solid transparent",
						color: navText,
					}}
					onMouseEnter={(e) => {
						e.currentTarget.style.backgroundColor = sidebarHover;
						e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
					}}
					onMouseLeave={(e) => {
						e.currentTarget.style.backgroundColor = navIdle;
						e.currentTarget.style.border = "1px solid transparent";
					}}
				>
					{isDark ? (
						<SunMedium className="w-4 h-4" />
					) : (
						<MoonStar className="w-4 h-4" />
					)}
				</Button>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					rounded="full"
					onClick={onLogout}
					aria-label="Logout"
					className="w-9 h-9 flex items-center justify-center transition-colors"
					style={{
						backgroundColor: navIdle,
						border: "1px solid transparent",
						color: navText,
					}}
					onMouseEnter={(e) => {
						e.currentTarget.style.backgroundColor = sidebarHover;
						e.currentTarget.style.border = `1px solid ${sidebarHoverBorder}`;
					}}
					onMouseLeave={(e) => {
						e.currentTarget.style.backgroundColor = navIdle;
						e.currentTarget.style.border = "1px solid transparent";
					}}
				>
					<LogOut className="w-4 h-4" />
				</Button>
			</div>
		</div>
	);
});
