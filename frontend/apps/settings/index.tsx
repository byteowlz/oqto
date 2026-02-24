"use client";

import { SettingsEditor } from "@/components/settings";
import { Button } from "@/components/ui/button";
import { useApp } from "@/hooks/use-app";
import { changePassword } from "@/lib/api/auth";
import { cn } from "@/lib/utils";
import {
	Brain,
	HelpCircle,
	Info,
	Keyboard,
	PanelLeftClose,
	PanelRightClose,
	Settings,
	User,
	X,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";

function SettingsHelpPanel({ locale }: { locale: "en" | "de" }) {
	const { t } = useTranslation();

	const tips = t("settings.helpTips", { returnObjects: true }) as string[];

	return (
		<div className="h-full overflow-y-auto p-4 space-y-6">
			<div>
				<h3 className="text-sm font-semibold mb-2 flex items-center gap-2">
					<Info className="w-4 h-4" />
					{t("settings.helpTitle")}
				</h3>
				<p className="text-sm text-muted-foreground">{t("settings.helpDescription")}</p>
			</div>

			<div>
				<h4 className="text-xs font-medium uppercase text-muted-foreground mb-2">
					Tips
				</h4>
				<ul className="space-y-2">
					{tips.map((tip) => (
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
					{t("settings.categories")}
				</h4>
				<p className="text-sm text-muted-foreground">{t("settings.categoryDescription")}</p>
			</div>
		</div>
	);
}

function ShortcutsPanel({ locale }: { locale: "en" | "de" }) {
	const { t } = useTranslation();

	const shortcuts = t("settings.shortcutsList", { returnObjects: true }) as Array<{ key: string; desc: string }>;

	return (
		<div className="h-full overflow-y-auto p-4 space-y-4">
			<h3 className="text-sm font-semibold flex items-center gap-2">
				<Keyboard className="w-4 h-4" />
				{t("settings.shortcutsTitle")}
			</h3>
			<div className="space-y-2">
				{shortcuts.map((shortcut) => (
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

function AccountPanel({ locale }: { locale: "en" | "de" }) {
	const { t } = useTranslation();
	const [currentPassword, setCurrentPassword] = useState("");
	const [newPassword, setNewPassword] = useState("");
	const [confirmPassword, setConfirmPassword] = useState("");
	const [error, setError] = useState<string | null>(null);
	const [success, setSuccess] = useState(false);
	const [loading, setLoading] = useState(false);

	const handleSubmit = useCallback(
		async (e: React.FormEvent) => {
			e.preventDefault();
			setError(null);
			setSuccess(false);

			if (newPassword !== confirmPassword) {
				setError(t("settings.passwordMismatch"));
				return;
			}
			if (newPassword.length < 8) {
				setError(t("settings.passwordTooShort"));
				return;
			}

			setLoading(true);
			try {
				await changePassword(currentPassword, newPassword);
				setSuccess(true);
				setCurrentPassword("");
				setNewPassword("");
				setConfirmPassword("");
			} catch (err) {
				setError(
					err instanceof Error ? err.message : "Failed to change password",
				);
			} finally {
				setLoading(false);
			}
		},
		[currentPassword, newPassword, confirmPassword, t],
	);

	return (
		<div className="h-full overflow-y-auto p-4 space-y-6">
			<div>
				<h3 className="text-sm font-semibold mb-4">{t("settings.changePassword")}</h3>
				<form onSubmit={handleSubmit} className="space-y-3 max-w-sm">
					<div>
						<label
							htmlFor="current-password"
							className="text-xs text-muted-foreground block mb-1"
						>
							{t("settings.currentPassword")}
						</label>
						<input
							id="current-password"
							type="password"
							value={currentPassword}
							onChange={(e) => setCurrentPassword(e.target.value)}
							className="w-full px-3 py-1.5 text-sm bg-background border border-border rounded focus:outline-none focus:ring-1 focus:ring-primary"
							required
							autoComplete="current-password"
						/>
					</div>
					<div>
						<label
							htmlFor="new-password"
							className="text-xs text-muted-foreground block mb-1"
						>
							{t("settings.newPassword")}
						</label>
						<input
							id="new-password"
							type="password"
							value={newPassword}
							onChange={(e) => setNewPassword(e.target.value)}
							className="w-full px-3 py-1.5 text-sm bg-background border border-border rounded focus:outline-none focus:ring-1 focus:ring-primary"
							required
							minLength={8}
							autoComplete="new-password"
						/>
					</div>
					<div>
						<label
							htmlFor="confirm-password"
							className="text-xs text-muted-foreground block mb-1"
						>
							{t("settings.confirmPassword")}
						</label>
						<input
							id="confirm-password"
							type="password"
							value={confirmPassword}
							onChange={(e) => setConfirmPassword(e.target.value)}
							className="w-full px-3 py-1.5 text-sm bg-background border border-border rounded focus:outline-none focus:ring-1 focus:ring-primary"
							required
							minLength={8}
							autoComplete="new-password"
						/>
					</div>
					{error && <p className="text-xs text-destructive">{error}</p>}
					{success && <p className="text-xs text-primary">{t("settings.passwordSuccess")}</p>}
					<button
						type="submit"
						disabled={loading}
						className="px-4 py-1.5 text-sm bg-primary text-primary-foreground rounded hover:bg-primary/90 disabled:opacity-50 transition-colors"
					>
						{loading ? "..." : t("settings.changePasswordSubmit")}
					</button>
				</form>
			</div>
		</div>
	);
}

export function SettingsApp() {
	const { locale, setActiveAppId } = useApp();
	const { t } = useTranslation();
	const navigate = useNavigate();
	const [mainTab, setMainTab] = useState<"oqto" | "mmry" | "account">("oqto");
	const [sidebarTab, setSidebarTab] = useState<"help" | "shortcuts">("help");
	const [mobileView, setMobileView] = useState<
		"oqto" | "mmry" | "account" | "help" | "shortcuts"
	>("oqto");
	const [rightSidebarCollapsed, setRightSidebarCollapsed] = useState(false);

	// TODO: Check if user is admin from context
	const isAdmin = true; // For now assume admin

	const handleClose = () => {
		setActiveAppId("sessions");
		navigate("/sessions");
	};

	return (
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4">
			{/* Mobile layout */}
			<div className="flex-1 min-h-0 flex flex-col lg:hidden">
				<div className="sticky top-0 z-10 bg-card border border-border rounded-t-xl overflow-hidden">
					<div className="flex gap-0.5 p-1 sm:p-2">
						<TabButton
							active={mobileView === "oqto"}
							onClick={() => {
								setMobileView("oqto");
								setMainTab("oqto");
							}}
							icon={Settings}
							label={t("settings.oqtoTab")}
						/>
						<TabButton
							active={mobileView === "mmry"}
							onClick={() => {
								setMobileView("mmry");
								setMainTab("mmry");
							}}
							icon={Brain}
							label={t("settings.mmryTab")}
						/>
						<TabButton
							active={mobileView === "account"}
							onClick={() => {
								setMobileView("account");
								setMainTab("account");
							}}
							icon={User}
							label={t("settings.accountTab")}
						/>
						<TabButton
							active={mobileView === "help"}
							onClick={() => {
								setMobileView("help");
								setSidebarTab("help");
							}}
							icon={HelpCircle}
							label={t("settings.help")}
						/>
						<TabButton
							active={mobileView === "shortcuts"}
							onClick={() => {
								setMobileView("shortcuts");
								setSidebarTab("shortcuts");
							}}
							icon={Keyboard}
							label={t("settings.shortcuts")}
						/>
					</div>
				</div>
				<div className="flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-3 sm:p-4 overflow-hidden flex flex-col gap-4">
					<div className="flex items-start justify-center gap-3 text-center">
						<div className="w-full">
							<h1 className="text-xl font-bold text-foreground tracking-wider">
								{t("settings.title")}
							</h1>
							<p className="text-sm text-muted-foreground">
								{t("settings.subtitle")}
							</p>
						</div>
					</div>
					<div className="flex-1 min-h-0 overflow-y-auto scrollbar-hide">
						{mobileView === "oqto" && (
							<div className="sm:max-w-3xl sm:mx-auto">
								<SettingsEditor app="oqto" isAdmin={isAdmin} />
							</div>
						)}
						{mobileView === "mmry" && (
							<div className="sm:max-w-3xl sm:mx-auto">
								<SettingsEditor app="mmry" isAdmin={isAdmin} />
							</div>
						)}
						{mobileView === "account" && (
							<div className="sm:max-w-3xl sm:mx-auto">
								<AccountPanel locale={locale} />
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
								{t("settings.title")}
							</h1>
							<p className="text-sm text-muted-foreground">
								{t("settings.subtitle")}
							</p>
						</div>
						<div className="flex items-center gap-2 text-xs text-muted-foreground">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={handleClose}
								className="items-center gap-1.5 text-muted-foreground hover:text-foreground"
								aria-label={t("common.close")}
							>
								<X className="w-4 h-4" />
								<span>{t("common.close")}</span>
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
							active={mainTab === "oqto"}
							onClick={() => setMainTab("oqto")}
							icon={Settings}
							label={t("settings.oqtoTab")}
						/>
						<TabButton
							active={mainTab === "mmry"}
							onClick={() => setMainTab("mmry")}
							icon={Brain}
							label={t("settings.mmryTab")}
						/>
						<TabButton
							active={mainTab === "account"}
							onClick={() => setMainTab("account")}
							icon={User}
							label={t("settings.accountTab")}
						/>
					</div>

					<div className="flex-1 min-h-0 overflow-y-auto scrollbar-hide mt-4">
						{mainTab === "oqto" && (
							<div className="max-w-3xl">
								<SettingsEditor app="oqto" isAdmin={isAdmin} />
							</div>
						)}
						{mainTab === "mmry" && (
							<div className="max-w-3xl">
								<SettingsEditor app="mmry" isAdmin={isAdmin} />
							</div>
						)}
						{mainTab === "account" && (
							<div className="max-w-3xl">
								<AccountPanel locale={locale} />
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
								aria-label={t("settings.help")}
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
								aria-label={t("settings.shortcuts")}
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
										title={t("settings.help")}
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
										title={t("settings.shortcuts")}
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
