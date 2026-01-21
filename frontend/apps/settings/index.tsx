"use client";

import { SettingsEditor } from "@/components/settings";
import { Button } from "@/components/ui/button";
import { useApp } from "@/hooks/use-app";
import { cn } from "@/lib/utils";
import {
	Brain,
	HelpCircle,
	Info,
	Keyboard,
	PanelLeftClose,
	PanelRightClose,
	Settings,
	X,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";

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

export function SettingsApp() {
	const { locale, setActiveAppId } = useApp();
	const navigate = useNavigate();
	const [mainTab, setMainTab] = useState<"octo" | "mmry">("octo");
	const [sidebarTab, setSidebarTab] = useState<"help" | "shortcuts">("help");
	const [mobileView, setMobileView] = useState<
		"octo" | "mmry" | "help" | "shortcuts"
	>("octo");
	const [rightSidebarCollapsed, setRightSidebarCollapsed] = useState(false);

	// TODO: Check if user is admin from context
	const isAdmin = true; // For now assume admin

	const handleClose = () => {
		setActiveAppId("sessions");
		navigate("/sessions");
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
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4">
			{/* Mobile layout */}
			<div className="flex-1 min-h-0 flex flex-col lg:hidden">
				<div className="sticky top-0 z-10 bg-card border border-border rounded-t-xl overflow-hidden">
					<div className="flex gap-0.5 p-1 sm:p-2">
						<TabButton
							active={mobileView === "octo"}
							onClick={() => {
								setMobileView("octo");
								setMainTab("octo");
							}}
							icon={Settings}
							label={t.octoTab}
						/>
						<TabButton
							active={mobileView === "mmry"}
							onClick={() => {
								setMobileView("mmry");
								setMainTab("mmry");
							}}
							icon={Brain}
							label={t.mmryTab}
						/>
						<TabButton
							active={mobileView === "help"}
							onClick={() => {
								setMobileView("help");
								setSidebarTab("help");
							}}
							icon={HelpCircle}
							label={t.help}
						/>
						<TabButton
							active={mobileView === "shortcuts"}
							onClick={() => {
								setMobileView("shortcuts");
								setSidebarTab("shortcuts");
							}}
							icon={Keyboard}
							label={t.shortcuts}
						/>
					</div>
				</div>
				<div className="flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-3 sm:p-4 overflow-hidden flex flex-col gap-4">
					<div className="flex items-start justify-center gap-3 text-center">
						<div className="w-full">
							<h1 className="text-xl font-bold text-foreground tracking-wider">
								{locale === "de" ? "EINSTELLUNGEN" : "SETTINGS"}
							</h1>
							<p className="text-sm text-muted-foreground">
								{locale === "de"
									? "Konfiguriere deine Arbeitsumgebung"
									: "Configure your workspace"}
							</p>
						</div>
					</div>
					<div className="flex-1 min-h-0 overflow-y-auto scrollbar-hide">
						{mobileView === "octo" && (
							<div className="sm:max-w-3xl sm:mx-auto">
								<SettingsEditor app="octo" isAdmin={isAdmin} />
							</div>
						)}
						{mobileView === "mmry" && (
							<div className="sm:max-w-3xl sm:mx-auto">
								<SettingsEditor app="mmry" isAdmin={isAdmin} />
							</div>
						)}
						{mobileView === "help" && <SettingsHelpPanel locale={locale} />}
						{mobileView === "shortcuts" && <ShortcutsPanel locale={locale} />}
					</div>
				</div>
			</div>

			{/* Desktop layout */}
			<div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
				<div className="flex-[3] min-w-0 bg-card border border-border p-4 xl:p-6 flex flex-col min-h-0 h-full">
					<div className="flex items-start justify-between gap-3">
						<div>
							<h1 className="text-xl md:text-2xl font-bold text-foreground tracking-wider">
								{locale === "de" ? "EINSTELLUNGEN" : "SETTINGS"}
							</h1>
							<p className="text-sm text-muted-foreground">
								{locale === "de"
									? "Konfiguriere deine Arbeitsumgebung"
									: "Configure your workspace"}
							</p>
						</div>
						<div className="flex items-center gap-2 text-xs text-muted-foreground">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={handleClose}
								className="items-center gap-1.5 text-muted-foreground hover:text-foreground"
								aria-label={t.close}
							>
								<X className="w-4 h-4" />
								<span>{t.close}</span>
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

					<div className="mt-4 flex items-center gap-2">
						<TabButton
							active={mainTab === "octo"}
							onClick={() => setMainTab("octo")}
							icon={Settings}
							label={t.octoTab}
						/>
						<TabButton
							active={mainTab === "mmry"}
							onClick={() => setMainTab("mmry")}
							icon={Brain}
							label={t.mmryTab}
						/>
					</div>

					<div className="flex-1 min-h-0 overflow-y-auto scrollbar-hide mt-4">
						{mainTab === "octo" && (
							<div className="max-w-3xl">
								<SettingsEditor app="octo" isAdmin={isAdmin} />
							</div>
						)}
						{mainTab === "mmry" && (
							<div className="max-w-3xl">
								<SettingsEditor app="mmry" isAdmin={isAdmin} />
							</div>
						)}
					</div>
				</div>

				<div
					className={cn(
						"bg-card border border-border flex flex-col min-h-0 h-full transition-all duration-200",
						rightSidebarCollapsed
							? "w-12 items-center"
							: "flex-[2] min-w-[280px] max-w-[360px]",
					)}
				>
					{rightSidebarCollapsed ? (
						<div className="flex flex-col gap-1 p-2 h-full overflow-y-auto">
							<button
								type="button"
								onClick={() => {
									setSidebarTab("help");
									setRightSidebarCollapsed(false);
								}}
								className={cn(
									"w-8 h-8 flex items-center justify-center relative transition-colors rounded",
									sidebarTab === "help"
										? "bg-primary/15 text-foreground border border-primary"
										: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
								)}
								aria-label={t.help}
							>
								<HelpCircle className="w-4 h-4" />
							</button>
							<button
								type="button"
								onClick={() => {
									setSidebarTab("shortcuts");
									setRightSidebarCollapsed(false);
								}}
								className={cn(
									"w-8 h-8 flex items-center justify-center relative transition-colors rounded",
									sidebarTab === "shortcuts"
										? "bg-primary/15 text-foreground border border-primary"
										: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
								)}
								aria-label={t.shortcuts}
							>
								<Keyboard className="w-4 h-4" />
							</button>
						</div>
					) : (
						<div className="flex flex-col h-full min-h-0">
							<div className="px-4 py-3 border-b border-border">
								<div className="flex items-center gap-2">
									<button
										type="button"
										onClick={() => setSidebarTab("help")}
										className={cn(
											"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
											sidebarTab === "help"
												? "bg-primary/15 text-foreground border border-primary"
												: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
										)}
										title={t.help}
									>
										<HelpCircle className="w-4 h-4" />
									</button>
									<button
										type="button"
										onClick={() => setSidebarTab("shortcuts")}
										className={cn(
											"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
											sidebarTab === "shortcuts"
												? "bg-primary/15 text-foreground border border-primary"
												: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
										)}
										title={t.shortcuts}
									>
										<Keyboard className="w-4 h-4" />
									</button>
								</div>
							</div>
							<div className="flex-1 min-h-0 overflow-y-auto">
								{sidebarTab === "help" && <SettingsHelpPanel locale={locale} />}
								{sidebarTab === "shortcuts" && (
									<ShortcutsPanel locale={locale} />
								)}
							</div>
						</div>
					)}
				</div>
			</div>
		</div>
	);
}

export default SettingsApp;
