/**
 * One file as placeable Content. It shows the file with the viewer its
 * format calls for, and for text it can switch to editing and write back
 * through the same host contract the Files pane uses.
 */

import { Pencil, RotateCcw, Save } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { FileHost } from "../platform/files-contract";
import { useFileDocument } from "./useFileDocument";
import { FileViewer } from "./viewers/FileViewer";
import { viewerFor } from "./viewers/registry";

interface FilePaneProps {
	readonly fileHost: FileHost;
	readonly workspacePath: string;
	/** Path relative to the work directory root. */
	readonly path: string;
}

export function FilePane({ fileHost, workspacePath, path }: FilePaneProps) {
	const { t } = useTranslation();
	const name = path.split("/").pop() ?? path;
	const document = useFileDocument(fileHost, workspacePath, path, name);
	const editable = viewerFor(name, false) === "text";
	return (
		<section className="wb-file" aria-label={name}>
			<header className="wb-file__bar">
				<span className="wb-file__name">{path}</span>
				{document.dirty ? (
					<span className="wb-file__dirty">{t("oqtoUi.file.unsaved")}</span>
				) : null}
				{editable && document.editing ? (
					<>
						<button
							type="button"
							className="wb-icon-button"
							aria-label={t("oqtoUi.file.save")}
							title={t("oqtoUi.file.save")}
							disabled={!document.dirty || document.status === "saving"}
							onClick={document.save}
						>
							<Save aria-hidden="true" />
						</button>
						<button
							type="button"
							className="wb-icon-button"
							aria-label={t("oqtoUi.file.discard")}
							title={t("oqtoUi.file.discard")}
							onClick={() => document.setEditing(false)}
						>
							<RotateCcw aria-hidden="true" />
						</button>
					</>
				) : null}
				{editable && !document.editing ? (
					<button
						type="button"
						className="wb-icon-button"
						aria-label={t("oqtoUi.file.edit")}
						title={t("oqtoUi.file.edit")}
						onClick={() => document.setEditing(true)}
					>
						<Pencil aria-hidden="true" />
					</button>
				) : null}
			</header>
			<div className="wb-file__body">
				{document.status === "failed" ? (
					<p className="wb-file__note">
						{t("oqtoUi.files.previewUnavailable")}
					</p>
				) : document.editing ? (
					<textarea
						className="wb-file__editor"
						aria-label={t("oqtoUi.file.edit")}
						spellCheck={false}
						value={document.text}
						onChange={(event) => document.edit(event.target.value)}
					/>
				) : (
					<FileViewer
						name={name}
						path={path}
						workspacePath={workspacePath}
						text={
							document.status === "loading"
								? { status: "loading" }
								: { status: "text", text: document.text }
						}
					/>
				)}
			</div>
		</section>
	);
}
