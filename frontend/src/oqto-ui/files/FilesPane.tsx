import {
	Braces,
	ChevronDown,
	FileCode2,
	FileImage,
	FileText,
	Folder,
	FolderPlus,
	FolderUp,
	Home,
	LayoutGrid,
	List,
	Upload,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { FileNode } from "../platform/contracts";

type FilesPaneProps = {
	files: FileNode[];
};

type FileKindIconProps = {
	kind: FileNode["kind"];
};

function FileKindIcon({ kind }: FileKindIconProps) {
	if (kind === "folder") return <Folder aria-hidden="true" />;
	if (kind === "typescript") return <FileCode2 aria-hidden="true" />;
	if (kind === "markdown") return <FileText aria-hidden="true" />;
	if (kind === "image") return <FileImage aria-hidden="true" />;
	return <Braces aria-hidden="true" />;
}

export function FilesPane({ files }: FilesPaneProps) {
	const { t } = useTranslation();
	return (
		<aside className="wb-panel" aria-label={t("oqtoUi.files.label")}>
			<div className="wb-files-toolbar">
				<button
					className="wb-icon-button"
					data-active="true"
					type="button"
					aria-label={t("oqtoUi.files.home")}
				>
					<Home aria-hidden="true" />
				</button>
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.files.parentFolder")}
				>
					<FolderUp aria-hidden="true" />
				</button>
				<span className="wb-files-toolbar__breadcrumb">
					{t("oqtoUi.files.rootLabel")}
				</span>
				<span className="wb-files-toolbar__spacer" />
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.files.upload")}
				>
					<Upload aria-hidden="true" />
				</button>
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.files.newFolder")}
				>
					<FolderPlus aria-hidden="true" />
				</button>
				<span className="wb-files-toolbar__divider" aria-hidden="true" />
				<button
					className="wb-icon-button"
					data-active="true"
					type="button"
					aria-label={t("oqtoUi.files.viewTree")}
				>
					<Folder aria-hidden="true" />
				</button>
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.files.viewList")}
				>
					<List aria-hidden="true" />
				</button>
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.files.viewGrid")}
				>
					<LayoutGrid aria-hidden="true" />
				</button>
			</div>

			<ul className="wb-tree" aria-label={t("oqtoUi.files.tree")}>
				{files.map((file) => (
					<li key={file.id}>
						<button
							className="wb-tree__row"
							data-changed={file.changed ? "true" : undefined}
							data-depth={file.depth}
							data-kind={file.kind}
							type="button"
						>
							{file.kind === "folder" ? (
								<ChevronDown aria-hidden="true" />
							) : (
								<span className="wb-tree__spacer" />
							)}
							<FileKindIcon kind={file.kind} />
							<span className="wb-tree__name">{file.name}</span>
							{file.count !== undefined ? (
								<span className="wb-tree__count">{file.count}</span>
							) : null}
						</button>
					</li>
				))}
			</ul>
		</aside>
	);
}
