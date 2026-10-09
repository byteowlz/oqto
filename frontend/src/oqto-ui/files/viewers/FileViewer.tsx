/**
 * Renders one file with the viewer its format calls for. Text arrives
 * through the host contract; image and media stream from the workspace
 * file URL so large files never pass through the engine.
 */

import { useTranslation } from "react-i18next";
import { workspaceFilePreviewUrl } from "../../platform/workspace-file-url";
import type { PreviewState } from "../usePreview";
import { mediaElement, viewerFor } from "./registry";

interface FileViewerProps {
	readonly name: string;
	readonly path: string;
	readonly workspacePath: string;
	/** Text arrives from the host; other viewers stream from a URL. */
	readonly text: PreviewState;
}

export function FileViewer({
	name,
	path,
	workspacePath,
	text,
}: FileViewerProps) {
	const { t } = useTranslation();
	const viewer = viewerFor(name, false);
	if (viewer === "image") {
		return (
			<img
				className="wb-files-preview__image"
				src={workspaceFilePreviewUrl(workspacePath, path)}
				alt={name}
			/>
		);
	}
	if (viewer === "pdf") {
		return (
			<iframe
				className="wb-files-preview__frame"
				title={name}
				src={workspaceFilePreviewUrl(workspacePath, path)}
			/>
		);
	}
	if (viewer === "media") {
		const url = workspaceFilePreviewUrl(workspacePath, path);
		return mediaElement(name) === "audio" ? (
			// biome-ignore lint/a11y/useMediaCaption: workspace media has no track.
			<audio className="wb-files-preview__media" controls src={url} />
		) : (
			// biome-ignore lint/a11y/useMediaCaption: workspace media has no track.
			<video className="wb-files-preview__media" controls src={url} />
		);
	}
	if (viewer === "text" && text.status === "text") {
		return <pre className="wb-files-preview__body">{text.text}</pre>;
	}
	return (
		<p className="wb-files-preview__body">
			{text.status === "loading"
				? t("oqtoUi.files.loading")
				: t("oqtoUi.files.previewUnavailable")}
		</p>
	);
}
