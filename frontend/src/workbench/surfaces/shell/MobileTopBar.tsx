import {
	ChevronDown,
	FileCode2,
	FileText,
	MessageSquare,
	PanelLeft,
	Terminal,
} from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
	LabNavigation,
	LabSession,
	LabWorkAreaTab,
	LabWorkDirectory,
} from "../../modules/lab/model";
import { TaskProgress } from "./TaskProgress";

type MobileTopBarProps = {
	directory: LabWorkDirectory;
	session: LabSession;
	activeView: string;
	activeTab: string;
	tabs: LabWorkAreaTab[];
	onOpenSessions: () => void;
	onNavigate: (next: LabNavigation) => void;
};

function tabIcon(id: LabWorkAreaTab["id"]) {
	if (id === "editor") return <FileCode2 aria-hidden="true" />;
	if (id === "terminal") return <Terminal aria-hidden="true" />;
	return <MessageSquare aria-hidden="true" />;
}

export function MobileTopBar({
	directory,
	session,
	activeView,
	activeTab,
	tabs,
	onOpenSessions,
	onNavigate,
}: MobileTopBarProps) {
	const { t } = useTranslation();
	const [menuOpen, setMenuOpen] = useState(false);
	const menuId = useId();
	const activeLabel =
		activeView === "files"
			? t("workbench.mobile.files")
			: (tabs.find((tab) => tab.id === activeTab)?.fileName ??
				t("workbench.mobile.chat"));

	const select = (next: LabNavigation) => {
		onNavigate(next);
		setMenuOpen(false);
	};

	return (
		<div className="wb-mobile-chrome">
			<button
				className="wb-mobile-chrome__sessions"
				type="button"
				aria-label={t("workbench.mobile.switchSession")}
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

			<TaskProgress tasks={session.tasks ?? []} placement="mobile" />

			<button
				className="wb-mobile-chrome__menu"
				type="button"
				aria-controls={menuId}
				aria-expanded={menuOpen}
				aria-label={`${t("workbench.mobile.label")}: ${activeLabel}`}
				onClick={() => setMenuOpen((current) => !current)}
			>
				{activeView === "files" ? (
					<FileText aria-hidden="true" />
				) : (
					tabIcon(activeTab as LabWorkAreaTab["id"])
				)}
				<ChevronDown aria-hidden="true" />
			</button>

			{menuOpen ? (
				<nav
					className="wb-mobile-tabs"
					id={menuId}
					aria-label={t("workbench.mobile.label")}
				>
					<button
						className="wb-mobile-tabs__item"
						data-active={activeView !== "files" && activeTab === "chat"}
						type="button"
						onClick={() => select({ mobileView: "chat", workAreaTab: "chat" })}
					>
						<MessageSquare aria-hidden="true" />
						<span>{t("workbench.mobile.chat")}</span>
					</button>
					<button
						className="wb-mobile-tabs__item"
						data-active={activeView === "files"}
						type="button"
						onClick={() => select({ mobileView: "files" })}
					>
						<FileText aria-hidden="true" />
						<span>{t("workbench.mobile.files")}</span>
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
								<span>{tab.fileName ?? t(`workbench.tools.${tab.id}`)}</span>
							</button>
						))}
				</nav>
			) : null}
		</div>
	);
}
