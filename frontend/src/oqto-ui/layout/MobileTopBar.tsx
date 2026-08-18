import {
	ChevronDown,
	FileCode2,
	FileText,
	Images,
	MessageSquare,
	PanelLeft,
	Terminal,
} from "lucide-react";
import { type ReactNode, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
	SessionOverview,
	UiNavigation,
	WorkAreaTab,
	WorkDirectory,
} from "../platform/contracts";

type MobileTopBarProps = {
	directory: WorkDirectory;
	session: SessionOverview;
	activeView: string;
	activeTab: string;
	tabs: WorkAreaTab[];
	taskProgress: ReactNode;
	onOpenSessions: () => void;
	onNavigate: (next: UiNavigation) => void;
};

function tabIcon(id: WorkAreaTab["id"]) {
	if (id === "editor") return <FileCode2 aria-hidden="true" />;
	if (id === "terminal") return <Terminal aria-hidden="true" />;
	if (id === "gallery") return <Images aria-hidden="true" />;
	return <MessageSquare aria-hidden="true" />;
}

export function MobileTopBar({
	directory,
	session,
	activeView,
	activeTab,
	tabs,
	taskProgress,
	onOpenSessions,
	onNavigate,
}: MobileTopBarProps) {
	const { t } = useTranslation();
	const [menuOpen, setMenuOpen] = useState(false);
	const menuId = useId();
	const activeLabel =
		activeView === "files"
			? t("oqtoUi.mobile.files")
			: (tabs.find((tab) => tab.id === activeTab)?.fileName ??
				t("oqtoUi.mobile.chat"));

	const select = (next: UiNavigation) => {
		onNavigate(next);
		setMenuOpen(false);
	};

	return (
		<div className="wb-mobile-chrome">
			<button
				className="wb-mobile-chrome__sessions"
				type="button"
				aria-label={t("oqtoUi.mobile.switchSession")}
				onClick={onOpenSessions}
			>
				<PanelLeft aria-hidden="true" />
			</button>

			<span className="wb-mobile-chrome__identity">
				<strong>{session.name}</strong>
				<small>
					{directory.name} [{session.id}]
				</small>
			</span>

			{taskProgress}

			<button
				className="wb-mobile-chrome__menu"
				type="button"
				aria-controls={menuId}
				aria-expanded={menuOpen}
				aria-label={`${t("oqtoUi.mobile.label")}: ${activeLabel}`}
				onClick={() => setMenuOpen((current) => !current)}
			>
				{activeView === "files" ? (
					<FileText aria-hidden="true" />
				) : (
					tabIcon(activeTab as WorkAreaTab["id"])
				)}
				<ChevronDown aria-hidden="true" />
			</button>

			{menuOpen ? (
				<nav
					className="wb-mobile-tabs"
					id={menuId}
					aria-label={t("oqtoUi.mobile.label")}
				>
					<button
						className="wb-mobile-tabs__item"
						data-active={activeView !== "files" && activeTab === "chat"}
						type="button"
						onClick={() => select({ mobileView: "chat", workAreaTab: "chat" })}
					>
						<MessageSquare aria-hidden="true" />
						<span>{t("oqtoUi.mobile.chat")}</span>
					</button>
					<button
						className="wb-mobile-tabs__item"
						data-active={activeView === "files"}
						type="button"
						onClick={() => select({ mobileView: "files" })}
					>
						<FileText aria-hidden="true" />
						<span>{t("oqtoUi.mobile.files")}</span>
					</button>
					{tabs
						.filter((tab) => tab.id !== "chat")
						.map((tab) => (
							<button
								className="wb-mobile-tabs__item"
								data-active={activeView !== "files" && activeTab === tab.id}
								key={tab.id}
								type="button"
								onClick={() =>
									select({ mobileView: "chat", workAreaTab: tab.id })
								}
							>
								{tabIcon(tab.id)}
								<span>{tab.fileName ?? t(`oqtoUi.tools.${tab.id}`)}</span>
							</button>
						))}
				</nav>
			) : null}
		</div>
	);
}
