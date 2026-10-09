/**
 * One input line serving filter, rename, and new folder, plus the delete
 * confirmation. Opened by a key, submitted with Enter, dismissed with
 * Escape — no modal dialogs.
 */

import { useTranslation } from "react-i18next";
import type { FileActions } from "./useFileActions";

interface FilesActionLineProps {
	readonly actions: FileActions;
	/** Name shown in the delete confirmation. */
	readonly target: string;
}

export function FilesActionLine({ actions, target }: FilesActionLineProps) {
	const { t } = useTranslation();
	if (actions.mode === null) return null;
	return actions.mode === "confirmDelete" ? (
		<div className="wb-files-filter wb-files-confirm">
			<span>{t("oqtoUi.files.deleteConfirm", { name: target })}</span>
			<button type="button" onClick={actions.submit}>
				{t("oqtoUi.files.delete")}
			</button>
			<button type="button" onClick={actions.cancel}>
				{t("common.cancel")}
			</button>
		</div>
	) : (
		// biome-ignore lint/a11y/noAutofocus: the action line is opened by a key and must receive it.
		<input
			className="wb-files-filter"
			type="text"
			autoFocus
			value={actions.draft}
			placeholder={t(
				actions.mode === "rename"
					? "oqtoUi.files.rename"
					: actions.mode === "create"
						? "oqtoUi.files.newFolderPrompt"
						: "oqtoUi.files.filter",
			)}
			aria-label={t(
				actions.mode === "rename"
					? "oqtoUi.files.rename"
					: actions.mode === "create"
						? "oqtoUi.files.newFolderPrompt"
						: "oqtoUi.files.filter",
			)}
			onChange={(event) => actions.change(event.target.value)}
			onKeyDown={(event) => {
				if (event.key === "Enter") {
					event.preventDefault();
					actions.submit();
				} else if (event.key === "Escape") {
					event.preventDefault();
					actions.cancel();
				}
			}}
		/>
	);
}
