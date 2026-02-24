import type { AppConfig } from "@/components/app-context";
import { Button } from "@/components/ui/button";
import type { ChatSession } from "@/lib/control-plane-client";
import {
	formatSessionDate,
	formatTempId,
	getDisplayPiTitle,
	getTempIdFromSession,
} from "@/lib/session-utils";
import { Menu, Plus } from "lucide-react";
import { memo } from "react";
import { useTranslation } from "react-i18next";

const sidebarBg = "var(--sidebar, #181b1a)";

export interface MobileHeaderProps {
	locale: string;
	isDark: boolean;
	activeAppId: string;
	activeApp: AppConfig | null;
	resolveText: (text: string | { en: string; de: string }) => string;
	selectedChatFromHistory: ChatSession | null;
	onMenuOpen: () => void;
	onNewChat: () => void;
}

export const MobileHeader = memo(function MobileHeader({
	locale,
	isDark,
	activeAppId,
	activeApp,
	resolveText,
	selectedChatFromHistory,
	onMenuOpen,
	onNewChat,
}: MobileHeaderProps) {
	const { t } = useTranslation();

	return (
		<header
			className="fixed top-0 left-0 right-0 flex items-center px-3 z-50 md:hidden h-[calc(3.5rem+env(safe-area-inset-top))]"
			style={{
				backgroundColor: sidebarBg,
				paddingTop: "env(safe-area-inset-top)",
			}}
		>
			<Button
				type="button"
				variant="ghost"
				size="icon"
				aria-label="Menu"
				onClick={onMenuOpen}
				className="text-muted-foreground hover:text-primary flex-shrink-0"
			>
				<Menu className="w-5 h-5" />
			</Button>
			{/* Header title */}
			{activeAppId === "sessions" ? (
				selectedChatFromHistory ? (
					<div className="flex-1 min-w-0 px-3 text-center">
						<div className="text-sm font-medium text-foreground truncate">
							{getDisplayPiTitle(selectedChatFromHistory)}
						</div>
						<div className="text-[10px] text-muted-foreground truncate">
							{(() => {
								const workspace = selectedChatFromHistory.workspace_path
									?.split("/")
									.filter(Boolean)
									.pop();
								const tempId = getTempIdFromSession(selectedChatFromHistory);
								const tempIdLabel = formatTempId(tempId);
								const date = selectedChatFromHistory.updated_at
									? formatSessionDate(selectedChatFromHistory.updated_at)
									: null;
								const parts: Array<{
									text: string;
									bold?: boolean;
									dim?: boolean;
								}> = [];
								if (workspace) parts.push({ text: workspace, bold: true });
								if (tempIdLabel) parts.push({ text: tempIdLabel });
								if (date) parts.push({ text: date, dim: true });
								return parts.map((part, i) => (
									<span key={part.text}>
										{i > 0 && <span className="opacity-60"> | </span>}
										<span
											className={
												part.bold ? "font-medium" : part.dim ? "opacity-60" : ""
											}
										>
											{part.text}
										</span>
									</span>
								));
							})()}
						</div>
					</div>
				) : (
					<div className="flex-1 flex justify-center">
						<img
							src={isDark ? "/oqto_logo_white.svg" : "/oqto_logo_black.svg"}
							alt="OQTO"
							width={80}
							height={32}
							className="h-8 w-auto object-contain"
						/>
					</div>
				)
			) : (
				<div className="flex-1 min-w-0 px-3 text-center">
					<div className="text-sm font-medium text-foreground truncate">
						{activeApp?.label ? resolveText(activeApp.label) : "Oqto"}
					</div>
					<div className="text-[10px] text-muted-foreground truncate">
						{activeApp?.description || ""}
					</div>
				</div>
			)}
			{/* New chat button */}
			{activeAppId === "sessions" && (
				<Button
					type="button"
					variant="ghost"
					size="icon"
					aria-label={t('sessions.newSession')}
					onClick={onNewChat}
					className="text-muted-foreground hover:text-primary flex-shrink-0"
				>
					<Plus className="w-5 h-5" />
				</Button>
			)}
		</header>
	);
});
