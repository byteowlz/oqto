"use client";

import { useApp } from "@/components/app-context";
import { SettingsEditor } from "@/components/settings";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Brain, Settings } from "lucide-react";
import { useState } from "react";

export function SettingsApp() {
	const { locale } = useApp();
	const [activeTab, setActiveTab] = useState("octo");

	// TODO: Check if user is admin from context
	const isAdmin = true; // For now assume admin

	const t = {
		en: {
			title: "Settings",
			octoTab: "Octo",
			mmryTab: "Memory",
			octoDesc: "Configure the Octo workspace platform",
			mmryDesc: "Configure memory service settings",
		},
		de: {
			title: "Einstellungen",
			octoTab: "Octo",
			mmryTab: "Speicher",
			octoDesc: "Octo Workspace-Plattform konfigurieren",
			mmryDesc: "Speicherdiensteinstellungen konfigurieren",
		},
	}[locale];

	return (
		<div className="h-full flex flex-col overflow-hidden">
			<div className="flex-1 overflow-auto p-3 sm:p-6">
				<div className="max-w-3xl mx-auto">
					<h1 className="text-xl sm:text-2xl font-bold mb-4 sm:mb-6">
						{t.title}
					</h1>

					<Tabs value={activeTab} onValueChange={setActiveTab}>
						<TabsList className="mb-4 sm:mb-6">
							<TabsTrigger value="octo" className="gap-1.5 sm:gap-2 text-sm">
								<Settings className="h-4 w-4" />
								{t.octoTab}
							</TabsTrigger>
							<TabsTrigger value="mmry" className="gap-1.5 sm:gap-2 text-sm">
								<Brain className="h-4 w-4" />
								{t.mmryTab}
							</TabsTrigger>
						</TabsList>

						<TabsContent value="octo">
							<SettingsEditor app="octo" title={t.octoDesc} isAdmin={isAdmin} />
						</TabsContent>

						<TabsContent value="mmry">
							<SettingsEditor app="mmry" title={t.mmryDesc} isAdmin={isAdmin} />
						</TabsContent>
					</Tabs>
				</div>
			</div>
		</div>
	);
}

export default SettingsApp;
