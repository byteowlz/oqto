"use client";

import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useAdminMetrics } from "@/hooks/use-admin";
import { useApp } from "@/hooks/use-app";
import { Activity, Shield, Ticket, Users, X } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { InviteCodesPanel } from "./InviteCodesPanel";
import { MetricsPanel } from "./MetricsPanel";
import { SessionsPanel } from "./SessionsPanel";
import { UsersPanel } from "./UsersPanel";

export function AdminApp() {
	const { metrics } = useAdminMetrics();
	const { setActiveAppId, locale } = useApp();
	const navigate = useNavigate();

	const handleClose = () => {
		setActiveAppId("sessions");
		navigate("/sessions");
	};

	return (
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 overflow-y-auto w-full">
			{/* Header with title and close button */}
			<div className="flex items-start justify-between mb-4">
				<div>
					<h1 className="text-xl md:text-2xl font-bold text-foreground tracking-wider">
						{locale === "de" ? "ADMIN DASHBOARD" : "ADMIN DASHBOARD"}
					</h1>
					<p className="text-sm text-muted-foreground">
						{locale === "de"
							? "Plattform-Monitoring und -Verwaltung"
							: "Platform monitoring and management"}
					</p>
				</div>
				{/* Close button - desktop only */}
				<Button
					type="button"
					variant="ghost"
					size="sm"
					onClick={handleClose}
					className="hidden md:flex items-center gap-1.5 text-muted-foreground hover:text-foreground"
					aria-label={locale === "de" ? "Schliessen" : "Close"}
				>
					<X className="w-4 h-4" />
					<span>{locale === "de" ? "Schliessen" : "Close"}</span>
				</Button>
			</div>

			<Tabs defaultValue="overview" className="flex-1">
				<TabsList className="w-full justify-start border-b border-border bg-transparent p-0 h-auto flex-wrap">
					<TabsTrigger
						value="overview"
						className="data-[state=active]:bg-muted border-b-2 border-transparent data-[state=active]:border-primary rounded-none px-4 py-2"
					>
						<Activity className="w-4 h-4 mr-2" />
						<span className="hidden sm:inline">Overview</span>
					</TabsTrigger>
					<TabsTrigger
						value="sessions"
						className="data-[state=active]:bg-muted border-b-2 border-transparent data-[state=active]:border-primary rounded-none px-4 py-2"
					>
						<Shield className="w-4 h-4 mr-2" />
						<span className="hidden sm:inline">Sessions</span>
					</TabsTrigger>
					<TabsTrigger
						value="users"
						className="data-[state=active]:bg-muted border-b-2 border-transparent data-[state=active]:border-primary rounded-none px-4 py-2"
					>
						<Users className="w-4 h-4 mr-2" />
						<span className="hidden sm:inline">Users</span>
					</TabsTrigger>
					<TabsTrigger
						value="invites"
						className="data-[state=active]:bg-muted border-b-2 border-transparent data-[state=active]:border-primary rounded-none px-4 py-2"
					>
						<Ticket className="w-4 h-4 mr-2" />
						<span className="hidden sm:inline">Invite Codes</span>
					</TabsTrigger>
				</TabsList>

				<TabsContent value="overview" className="mt-4 space-y-4">
					<MetricsPanel />
					<SessionsPanel containerStats={metrics?.containers} />
				</TabsContent>

				<TabsContent value="sessions" className="mt-4">
					<SessionsPanel containerStats={metrics?.containers} />
				</TabsContent>

				<TabsContent value="users" className="mt-4">
					<UsersPanel />
				</TabsContent>

				<TabsContent value="invites" className="mt-4">
					<InviteCodesPanel />
				</TabsContent>
			</Tabs>
		</div>
	);
}

export default AdminApp;
