"use client";

import { SettingsEditor } from "@/components/settings";
import { Button } from "@/components/ui/button";
import { useApp } from "@/hooks/use-app";
import { cn } from "@/lib/utils";
import { Brain, HelpCircle, Info, Keyboard, Settings, X } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useState } from "react";

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

interface TabButtonProps {
	active: boolean;
	onClick: () => void;
	icon: LucideIcon;
	label: string;
}

function TabButton({ active, onClick, icon: Icon, label }: TabButtonProps) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={cn(
				"flex items-center justify-center gap-1.5 px-3 py-1.5 text-xs font-medium transition-colors rounded-md",
				active
					? "bg-primary text-primary-foreground"
					: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
			)}
		>
			<Icon className="w-3.5 h-3.5" />
			<span>{label}</span>
		</button>
	);
}

export function SettingsApp() {
	const { locale, setActiveAppId } = useApp();
	const [activeTab, setActiveTab] = useState<
		"octo" | "mmry" | "help" | "shortcuts"
	>("octo");

	// TODO: Check if user is admin from context
	const isAdmin = true; // For now assume admin

	const handleClose = () => {
		setActiveAppId("sessions");
	};

	const t = {
		en: {
			octoTab: "Octo",
			mmryTab: "Memory",
			help: "Help",
			shortcuts: "Keys",
			close: "Close",
		},
		de: {
			octoTab: "Octo",
			mmryTab: "Speicher",
			help: "Hilfe",
			shortcuts: "Tasten",
			close: "Schliessen",
		},
	}[locale];

	return (
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6">
			{/* Unified tab bar */}
			<div className="bg-card border border-border rounded-t-xl sm:rounded-xl sm:mb-4 px-2 py-1.5 flex items-center gap-1 overflow-x-auto scrollbar-hide">
				{/* Close button - desktop only */}
				<Button
					type="button"
					variant="ghost"
					size="sm"
					onClick={handleClose}
					className="hidden md:flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground mr-2"
					aria-label={t.close}
				>
					<X className="w-3.5 h-3.5" />
					<span>{t.close}</span>
				</Button>
				<div className="hidden md:block w-px h-5 bg-border mr-1" />
				<TabButton
					active={activeTab === "octo"}
					onClick={() => setActiveTab("octo")}
					icon={Settings}
					label={t.octoTab}
				/>
				<TabButton
					active={activeTab === "mmry"}
					onClick={() => setActiveTab("mmry")}
					icon={Brain}
					label={t.mmryTab}
				/>
				<div className="w-px h-5 bg-border mx-1" />
				<TabButton
					active={activeTab === "help"}
					onClick={() => setActiveTab("help")}
					icon={HelpCircle}
					label={t.help}
				/>
				<TabButton
					active={activeTab === "shortcuts"}
					onClick={() => setActiveTab("shortcuts")}
					icon={Keyboard}
					label={t.shortcuts}
				/>
			</div>

			{/* Content area */}
			<div className="flex-1 min-h-0 bg-card border border-border border-t-0 sm:border-t rounded-b-xl sm:rounded-xl overflow-hidden">
				<div className="h-full overflow-y-auto scrollbar-hide">
					{activeTab === "octo" && (
						<div className="sm:max-w-3xl sm:mx-auto">
							<SettingsEditor app="octo" isAdmin={isAdmin} />
						</div>
					)}
					{activeTab === "mmry" && (
						<div className="sm:max-w-3xl sm:mx-auto">
							<SettingsEditor app="mmry" isAdmin={isAdmin} />
						</div>
					)}
					{activeTab === "help" && <SettingsHelpPanel locale={locale} />}
					{activeTab === "shortcuts" && <ShortcutsPanel locale={locale} />}
				</div>
			</div>
		</div>
	);
}

export default SettingsApp;
