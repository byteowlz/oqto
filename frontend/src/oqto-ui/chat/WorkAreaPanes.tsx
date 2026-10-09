import { Download, FileCode2, PanelRightClose, Search } from "lucide-react";
import { useTranslation } from "react-i18next";

interface ChatActionsProps {
	/** Toggles the side panel Container; absent when the host cannot. */
	readonly onTogglePanel?: () => void;
}

/**
 * The Chat presentation's own actions. Choosing what a Container shows is
 * the compositor's tab bar's job, so this row carries no tabs of its own.
 */
export function ChatActions({ onTogglePanel }: ChatActionsProps) {
	const { t } = useTranslation();
	return (
		<>
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
				onClick={onTogglePanel}
			>
				<PanelRightClose aria-hidden="true" />
			</button>
		</>
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
