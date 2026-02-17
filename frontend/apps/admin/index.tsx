"use client";

import { Button } from "@/components/ui/button";
import { useApp } from "@/hooks/use-app";
import { cn } from "@/lib/utils";
import {
	Activity,
	Blocks,
	PanelLeftClose,
	PanelRightClose,
	Shield,
	Ticket,
	Users,
	X,
} from "lucide-react";
import { Suspense, lazy, useState } from "react";
import { useNavigate } from "react-router-dom";
import { InviteCodesPanel } from "./InviteCodesPanel";
import { MetricsPanel } from "./MetricsPanel";
import { ModelsPanel } from "./ModelsPanel";
import { SessionsPanel } from "./SessionsPanel";
import { UsersPanel } from "./UsersPanel";

import type { LucideIcon } from "lucide-react";

function SectionTabButton({
	active,
	onClick,
	icon: Icon,
	label,
}: {
	active: boolean;
	onClick: () => void;
	icon: LucideIcon;
	label: string;
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
			<Icon className="w-4 h-4" />
		</button>
	);
}

const TerminalView = lazy(() =>
	import("@/features/sessions/components/TerminalView").then((mod) => ({
		default: mod.TerminalView,
	})),
);

export function AdminApp() {
	const { setActiveAppId, locale } = useApp();
	const navigate = useNavigate();
	const [activeSection, setActiveSection] = useState<
		"overview" | "sessions" | "users" | "invites" | "models"
	>("overview");
	const [rightSidebarCollapsed, setRightSidebarCollapsed] = useState(false);

	const handleClose = () => {
		setActiveAppId("sessions");
		navigate("/sessions");
	};

	const labels = {
		overview: locale === "de" ? "Ubersicht" : "Overview",
		sessions: locale === "de" ? "Sessions" : "Sessions",
		users: locale === "de" ? "Benutzer" : "Users",
		invites: locale === "de" ? "Einladungen" : "Invite Codes",
		models: locale === "de" ? "Modelle" : "Models",
		close: locale === "de" ? "Schliessen" : "Close",
		title: locale === "de" ? "ADMIN DASHBOARD" : "ADMIN DASHBOARD",
		subtitle:
			locale === "de"
				? "Plattform-Monitoring und -Verwaltung"
				: "Platform monitoring and management",
	};

	const renderContent = () => {
		switch (activeSection) {
			case "overview":
				return (
					<div className="space-y-4">
						<MetricsPanel />
						<SessionsPanel containerStats={undefined} />
					</div>
				);
			case "sessions":
				return <SessionsPanel containerStats={undefined} />;
			case "users":
				return <UsersPanel />;
			case "invites":
				return <InviteCodesPanel />;
			case "models":
				return <ModelsPanel />;
			default:
				return null;
		}
	};

	return (
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4 w-full">
			{/* Mobile layout */}
			<div className="flex-1 min-h-0 flex flex-col lg:hidden">
				<div className="sticky top-0 z-10 bg-card border border-border rounded-t-xl overflow-hidden">
					<div className="flex gap-0.5 p-1 sm:p-2">
						<SectionTabButton
							active={activeSection === "overview"}
							onClick={() => setActiveSection("overview")}
							icon={Activity}
							label={labels.overview}
						/>
						<SectionTabButton
							active={activeSection === "sessions"}
							onClick={() => setActiveSection("sessions")}
							icon={Shield}
							label={labels.sessions}
						/>
						<SectionTabButton
							active={activeSection === "users"}
							onClick={() => setActiveSection("users")}
							icon={Users}
							label={labels.users}
						/>
						<SectionTabButton
							active={activeSection === "invites"}
							onClick={() => setActiveSection("invites")}
							icon={Ticket}
							label={labels.invites}
						/>
						<SectionTabButton
							active={activeSection === "models"}
							onClick={() => setActiveSection("models")}
							icon={Blocks}
							label={labels.models}
						/>
					</div>
				</div>
				<div className="flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-3 sm:p-4 overflow-hidden flex flex-col gap-4">
					<div className="w-full text-center">
						<h1 className="text-xl font-bold text-foreground tracking-wider">
							{labels.title}
						</h1>
						<p className="text-sm text-muted-foreground">{labels.subtitle}</p>
					</div>
					<div className="flex-1 min-h-0 overflow-y-auto scrollbar-hide">
						{renderContent()}
					</div>
				</div>
			</div>

			{/* Desktop layout */}
			<div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
				<div className="flex-[3] min-w-0 bg-card border border-border p-4 xl:p-6 flex flex-col min-h-0 h-full">
					<div className="flex items-start justify-between gap-3">
						<div>
							<h1 className="text-xl md:text-2xl font-bold text-foreground tracking-wider">
								{labels.title}
							</h1>
							<p className="text-sm text-muted-foreground">{labels.subtitle}</p>
						</div>
						<div className="flex items-center gap-2 text-xs text-muted-foreground">
							<div className="flex items-center gap-1">
								<SectionTabButton
									active={activeSection === "overview"}
									onClick={() => setActiveSection("overview")}
									icon={Activity}
									label={labels.overview}
								/>
								<SectionTabButton
									active={activeSection === "sessions"}
									onClick={() => setActiveSection("sessions")}
									icon={Shield}
									label={labels.sessions}
								/>
								<SectionTabButton
									active={activeSection === "users"}
									onClick={() => setActiveSection("users")}
									icon={Users}
									label={labels.users}
								/>
								<SectionTabButton
									active={activeSection === "invites"}
									onClick={() => setActiveSection("invites")}
									icon={Ticket}
									label={labels.invites}
								/>
								<SectionTabButton
									active={activeSection === "models"}
									onClick={() => setActiveSection("models")}
									icon={Blocks}
									label={labels.models}
								/>
							</div>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={handleClose}
								className="items-center gap-1.5 text-muted-foreground hover:text-foreground"
								aria-label={labels.close}
							>
								<X className="w-4 h-4" />
								<span>{labels.close}</span>
							</Button>
							<button
								type="button"
								onClick={() => setRightSidebarCollapsed((prev) => !prev)}
								className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
								title={
									rightSidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"
								}
							>
								{rightSidebarCollapsed ? (
									<PanelLeftClose className="w-4 h-4" />
								) : (
									<PanelRightClose className="w-4 h-4" />
								)}
							</button>
						</div>
					</div>

					<div className="flex-1 min-h-0 overflow-y-auto scrollbar-hide mt-4">
						{renderContent()}
					</div>
				</div>

				<div
					className={cn(
						"bg-card border border-border flex flex-col min-h-0 h-full transition-all duration-200",
						rightSidebarCollapsed
							? "w-12 items-center"
							: "flex-[2] min-w-[300px] max-w-[380px]",
					)}
				>
					{rightSidebarCollapsed ? (
						<div className="flex flex-1 items-start justify-center pt-3">
							<button
								type="button"
								onClick={() => setRightSidebarCollapsed(false)}
								className="w-8 h-8 flex items-center justify-center relative transition-colors rounded text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50"
								aria-label="Expand sidebar"
							>
								<PanelLeftClose className="w-4 h-4" />
							</button>
						</div>
					) : (
						<div className="flex flex-col h-full min-h-0">
							<div className="px-4 py-3 border-b border-border">
								<div className="text-xs uppercase tracking-wider text-muted-foreground">
									{locale === "de" ? "Systemstatus" : "System status"}
								</div>
							</div>
							<div className="flex-1 min-h-0 overflow-hidden">
								<div className="h-full">
									<Suspense
										fallback={
											<div className="p-3 text-xs text-muted-foreground">
												Loading terminal...
											</div>
										}
									>
										<TerminalView workspacePath="." />
									</Suspense>
								</div>
							</div>
						</div>
					)}
				</div>
			</div>
		</div>
	);
}

export default AdminApp;
