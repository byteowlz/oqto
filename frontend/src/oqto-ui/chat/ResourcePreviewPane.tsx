import { ResourcePreviewHost } from "@/components/viewers/resource-preview-host";
import { useDocumentEvent } from "@/hooks/use-document-event";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { workspaceFilePreviewUrl } from "../platform/workspace-file-url";

export type PreviewSelection = {
	path: string;
	range?: { startLine?: number; endLine?: number };
};

type ResourcePreviewPaneProps = {
	selection: PreviewSelection;
	workspacePath: string;
	onClose: () => void;
};

export function ResourcePreviewPane({
	selection,
	workspacePath,
	onClose,
}: ResourcePreviewPaneProps) {
	const { t } = useTranslation();
	useDocumentEvent(
		"keydown",
		(event) => {
			if (event.key === "Escape") onClose();
		},
		true,
	);
	const range = selection.range;
	const rangeLabel = range?.startLine
		? `:${range.startLine}${range.endLine && range.endLine !== range.startLine ? `-${range.endLine}` : ""}`
		: "";
	return (
		<section
			className="wb-resource-preview"
			aria-label={t("oqtoUi.preview.label", { path: selection.path })}
		>
			<header className="wb-resource-preview__header">
				<strong>
					{selection.path}
					{rangeLabel}
				</strong>
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.preview.close")}
					onClick={onClose}
				>
					<X aria-hidden="true" />
				</button>
			</header>
			<div className="wb-resource-preview__body">
				<ResourcePreviewHost
					resource={{
						kind: "workspace-file",
						uri: workspaceFilePreviewUrl(workspacePath, selection.path),
						label: selection.path.split("/").at(-1) ?? selection.path,
						range: selection.range,
					}}
				/>
			</div>
		</section>
	);
}
