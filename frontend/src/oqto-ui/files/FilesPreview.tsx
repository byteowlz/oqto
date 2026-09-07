/**
 * Quick Look panel: the cursor entry's facts and, for textual kinds, its
 * contents. Opened with Space, closed with Space or Escape.
 */

import { useTranslation } from "react-i18next";
import type { FileEntry } from "../platform/files-contract";
import type { PreviewState } from "./usePreview";

interface FilesPreviewProps {
	readonly entry: FileEntry;
	readonly preview: PreviewState;
	readonly size: string;
	readonly modified: string;
}

export function FilesPreview({
	entry,
	preview,
	size,
	modified,
}: FilesPreviewProps) {
	const { t } = useTranslation();
	return (
		<section
			className="wb-files-preview"
			aria-label={t("oqtoUi.files.preview")}
		>
			<span className="wb-files-preview__name">{entry.name}</span>
			<span className="wb-files-preview__facts">
				{size === "" ? null : <span>{size}</span>}
				{modified === "" ? null : <span>{modified}</span>}
			</span>
			{preview.status === "text" ? (
				<pre className="wb-files-preview__body">{preview.text}</pre>
			) : (
				<p className="wb-files-preview__body">
					{preview.status === "loading"
						? t("oqtoUi.files.loading")
						: t("oqtoUi.files.previewUnavailable")}
				</p>
			)}
		</section>
	);
}
