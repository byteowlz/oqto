"use client";

import {
	AppContentLayout,
	type SidebarTab,
} from "@/components/app-content-layout";
import { SettingsEditor } from "@/components/settings";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useApp } from "@/hooks/use-app";
import { Brain, HelpCircle, Info, Keyboard, Settings } from "lucide-react";
import { useMemo, useState } from "react";

function SettingsHelpPanel({ locale }: { locale: "en" | "de" }) {
	const t = {
		en: {
			title: "Settings Help",
			description:
				"Configure your Octo workspace and memory service settings here.",
			tips: [
				"Changes are applied when you click Save",
				"Click the reset button next to a field to restore its default value",
				"Sensitive values like API keys are hidden by default",
				"Settings marked as 'configured' have been customized",
			],
			categories: "Settings Categories",
			categoryDesc:
				"Settings are organized into collapsible sections. Click on a section header to expand or collapse it.",
		},
		de: {
			title: "Einstellungen Hilfe",
			description:
				"Konfigurieren Sie hier Ihren Octo-Arbeitsbereich und die Speicherdiensteinstellungen.",
			tips: [
				"Anderungen werden angewendet, wenn Sie auf Speichern klicken",
				"Klicken Sie auf die Reset-Schaltflache neben einem Feld, um den Standardwert wiederherzustellen",
				"Sensible Werte wie API-Schlussel sind standardmassig ausgeblendet",
				"Als 'konfiguriert' markierte Einstellungen wurden angepasst",
			],
			categories: "Einstellungskategorien",
			categoryDesc:
				"Die Einstellungen sind in einklappbare Abschnitte unterteilt. Klicken Sie auf einen Abschnittskopf, um ihn ein- oder auszuklappen.",
		},
	}[locale];

	return (
		<div className="h-full overflow-y-auto p-4 space-y-6">
			<div>
				<h3 className="text-sm font-semibold mb-2 flex items-center gap-2">
					<Info className="w-4 h-4" />
					{t.title}
				</h3>
				<p className="text-sm text-muted-foreground">{t.description}</p>
			</div>

			<div>
				<h4 className="text-xs font-medium uppercase text-muted-foreground mb-2">
					Tips
				</h4>
				<ul className="space-y-2">
					{t.tips.map((tip) => (
						<li
							key={tip}
							className="text-sm text-muted-foreground flex items-start gap-2"
						>
							<span className="text-primary mt-0.5">-</span>
							{tip}
						</li>
					))}
				</ul>
			</div>

			<div>
				<h4 className="text-xs font-medium uppercase text-muted-foreground mb-2">
					{t.categories}
				</h4>
				<p className="text-sm text-muted-foreground">{t.categoryDesc}</p>
			</div>
		</div>
	);
}

function ShortcutsPanel({ locale }: { locale: "en" | "de" }) {
	const t = {
		en: {
			title: "Keyboard Shortcuts",
			shortcuts: [
				{ key: "Ctrl + S", desc: "Save changes" },
				{ key: "Ctrl + K", desc: "Open command palette" },
				{ key: "Escape", desc: "Close dialogs" },
			],
		},
		de: {
			title: "Tastenkurzel",
			shortcuts: [
				{ key: "Strg + S", desc: "Anderungen speichern" },
				{ key: "Strg + K", desc: "Befehlspalette offnen" },
				{ key: "Escape", desc: "Dialoge schliessen" },
			],
		},
	}[locale];

	return (
		<div className="h-full overflow-y-auto p-4 space-y-4">
			<h3 className="text-sm font-semibold flex items-center gap-2">
				<Keyboard className="w-4 h-4" />
				{t.title}
			</h3>
			<div className="space-y-2">
				{t.shortcuts.map((shortcut) => (
					<div
						key={shortcut.key}
						className="flex items-center justify-between text-sm py-1.5 border-b border-border/50 last:border-0"
					>
						<span className="text-muted-foreground">{shortcut.desc}</span>
						<kbd className="px-2 py-0.5 bg-muted rounded text-xs font-mono">
							{shortcut.key}
						</kbd>
					</div>
				))}
			</div>
		</div>
	);
}

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
			help: "Help",
			shortcuts: "Shortcuts",
		},
		de: {
			title: "Einstellungen",
			octoTab: "Octo",
			mmryTab: "Speicher",
			octoDesc: "Octo Workspace-Plattform konfigurieren",
			mmryDesc: "Speicherdiensteinstellungen konfigurieren",
			help: "Hilfe",
			shortcuts: "Tastenkurzel",
		},
	}[locale];

	const sidebarTabs: SidebarTab[] = useMemo(
		() => [
			{
				id: "help",
				label: t.help,
				icon: HelpCircle,
				content: <SettingsHelpPanel locale={locale} />,
			},
			{
				id: "shortcuts",
				label: t.shortcuts,
				icon: Keyboard,
				content: <ShortcutsPanel locale={locale} />,
			},
		],
		[locale, t.help, t.shortcuts],
	);

	const header = (
		<div className="flex items-center justify-between pb-3 mb-3 border-b border-border">
			<h1 className="text-lg sm:text-xl font-semibold">{t.title}</h1>
			<Tabs value={activeTab} onValueChange={setActiveTab}>
				<TabsList className="h-8">
					<TabsTrigger
						value="octo"
						className="gap-1.5 text-xs sm:text-sm h-7 px-2 sm:px-3"
					>
						<Settings className="h-3.5 w-3.5" />
						<span className="hidden sm:inline">{t.octoTab}</span>
					</TabsTrigger>
					<TabsTrigger
						value="mmry"
						className="gap-1.5 text-xs sm:text-sm h-7 px-2 sm:px-3"
					>
						<Brain className="h-3.5 w-3.5" />
						<span className="hidden sm:inline">{t.mmryTab}</span>
					</TabsTrigger>
				</TabsList>
			</Tabs>
		</div>
	);

	return (
		<AppContentLayout
			mainTabLabel={t.title}
			mainTabIcon={Settings}
			sidebarTabs={sidebarTabs}
			defaultSidebarTab="help"
			header={header}
		>
			<div className="flex-1 min-h-0 overflow-y-auto scrollbar-hide">
				<div className="max-w-3xl mx-auto">
					<Tabs value={activeTab}>
						<TabsContent value="octo" className="mt-0">
							<SettingsEditor app="octo" title={t.octoDesc} isAdmin={isAdmin} />
						</TabsContent>

						<TabsContent value="mmry" className="mt-0">
							<SettingsEditor app="mmry" title={t.mmryDesc} isAdmin={isAdmin} />
						</TabsContent>
					</Tabs>
				</div>
			</div>
		</AppContentLayout>
	);
}

export default SettingsApp;
