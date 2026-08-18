import {
	Download,
	FileCode2,
	Folder,
	Images,
	MessageSquare,
	PanelRightClose,
	Pin,
	Plus,
	Search,
	Terminal,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { UiNavigation, WorkAreaTab } from "../platform/contracts";

type WorkAreaTabsProps = {
	tabs: WorkAreaTab[];
	activeTab: string;
	chatLabel: string;
	chatMeta: string;
	onNavigate: (next: UiNavigation) => void;
};

function tabIcon(id: WorkAreaTab["id"]) {
	if (id === "editor") return <FileCode2 aria-hidden="true" />;
	if (id === "terminal") return <Terminal aria-hidden="true" />;
	if (id === "gallery") return <Images aria-hidden="true" />;
	return <MessageSquare aria-hidden="true" />;
}

export function WorkAreaTabs({
	tabs,
	activeTab,
	chatLabel,
	chatMeta,
	onNavigate,
}: WorkAreaTabsProps) {
	const { t } = useTranslation();
	return (
		<div
			className="wb-wa-tabs"
			role="tablist"
			aria-label={t("oqtoUi.workArea.label")}
		>
			{tabs.map((tab) => {
				const label =
					tab.id === "chat"
						? chatLabel
						: (tab.fileName ?? t(`oqtoUi.tools.${tab.id}`));
				const ownership = t(
					tab.owner === "session"
						? "oqtoUi.workArea.sessionOwned"
						: "oqtoUi.workArea.workDirectoryOwned",
				);
				return (
					<button
						aria-selected={activeTab === tab.id}
						className="wb-wa-tab"
						data-active={activeTab === tab.id}
						data-owner={tab.owner}
						key={tab.id}
						role="tab"
						title={tab.id === "chat" ? chatMeta : ownership}
						type="button"
						onClick={() => onNavigate({ workAreaTab: tab.id })}
					>
						{tabIcon(tab.id)}
						<span className="wb-wa-tab__label">{label}</span>
						{tab.owner === "workDirectory" ? (
							<Folder className="wb-wa-tab__owner" aria-hidden="true" />
						) : null}
						{tab.pinned ? (
							<Pin
								className="wb-wa-tab__pin"
								aria-label={t("oqtoUi.workArea.pinned")}
							/>
						) : null}
					</button>
				);
			})}
			<button
				className="wb-wa-tab wb-wa-tab--new"
				type="button"
				aria-label={t("oqtoUi.workArea.openTool")}
			>
				<Plus aria-hidden="true" />
			</button>
			<span className="wb-wa-tabs__spacer" />
			<button
				className="wb-icon-button"
				type="button"
				aria-label={t("oqtoUi.chat.download")}
			>
				<Download aria-hidden="true" />
			</button>
			<button
				className="wb-icon-button"
				type="button"
				aria-label={t("oqtoUi.chat.search")}
			>
				<Search aria-hidden="true" />
			</button>
			<button
				className="wb-icon-button"
				type="button"
				aria-label={t("oqtoUi.chat.togglePanel")}
			>
				<PanelRightClose aria-hidden="true" />
			</button>
		</div>
	);
}

type EditorPaneProps = {
	fileName: string;
	lines: string[];
};

export function EditorPane({ fileName, lines }: EditorPaneProps) {
	const { t } = useTranslation();
	return (
		<section className="wb-editor" aria-label={fileName}>
			<header className="wb-editor__header">
				<FileCode2 aria-hidden="true" />
				<span>{fileName}</span>
				<span className="wb-editor__scope">
					{t("oqtoUi.workArea.workDirectoryOwned")}
				</span>
			</header>
			<div className="wb-editor__body">
				{lines.map((line, index) => (
					<div className="wb-editor__line" key={`${index}-${line}`}>
						<span className="wb-editor__number">{index + 1}</span>
						<code>{line || " "}</code>
					</div>
				))}
			</div>
		</section>
	);
}

type TerminalPaneProps = {
	lines: string[];
};

export function TerminalPane({ lines }: TerminalPaneProps) {
	const { t } = useTranslation();
	return (
		<section className="wb-terminal" aria-label={t("oqtoUi.tools.terminal")}>
			{lines.map((line, index) => (
				<div
					className="wb-terminal__line"
					data-prompt={line.startsWith("$")}
					key={`${index}-${line}`}
				>
					{line}
				</div>
			))}
		</section>
	);
}
