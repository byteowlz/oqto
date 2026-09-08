/**
 * Miller columns: parent, current directory, and a preview of what the
 * cursor points at — a child listing for directories, Quick Look contents
 * for files. Shown only when the pane is allocated enough width
 * (ADR-0037 progressive fidelity); the engine and rows are the same.
 */

import { useTranslation } from "react-i18next";
import type { FileEntry } from "../platform/files-contract";
import { type EntryFacts, FilesList } from "./FilesList";
import { FilesPreview } from "./FilesPreview";
import { parentPath } from "./entries";
import type { FilesState, ListingState } from "./navigator";
import type { PreviewState } from "./usePreview";

/** Row rendering shared by every column, passed as one value. */
export interface ColumnRendering {
	readonly marks: {
		readonly selection: ReadonlySet<string>;
		readonly changed: ReadonlySet<string>;
	};
	readonly facts: (entry: FileEntry) => EntryFacts;
	readonly onSelect: (path: string, event: React.MouseEvent) => void;
	readonly onOpen: (entry: FileEntry) => void;
	readonly scrollRef: (scrollToCursor: () => void) => void;
}

interface MillerColumnsProps {
	readonly state: FilesState;
	readonly entries: readonly FileEntry[];
	readonly focused: FileEntry | null;
	readonly preview: PreviewState;
	readonly rendering: ColumnRendering;
}

function readyEntries(listing: ListingState | undefined): readonly FileEntry[] {
	return listing?.status === "ready" ? listing.entries : [];
}

export function MillerColumns({
	state,
	entries,
	focused,
	preview,
	rendering,
}: MillerColumnsProps) {
	const { t } = useTranslation();
	const { marks, facts, onSelect, onOpen, scrollRef } = rendering;
	const parent = parentPath(state.cwd);
	const noFacts = () => ({ size: "", modified: "" });
	const childEntries = focused?.directory
		? readyEntries(state.listings[focused.path])
		: [];
	return (
		<div className="wb-files-columns">
			{parent === null ? null : (
				<div className="wb-files-column" data-role="parent">
					<FilesList
						rows={readyEntries(state.listings[parent]).map((entry) => ({
							entry,
							depth: 0,
							expanded: false,
						}))}
						cursor={state.cwd}
						marks={marks}
						facts={noFacts}
						onSelect={(path) =>
							onOpen({ ...(focused as FileEntry), path, directory: true })
						}
						onOpen={onOpen}
					/>
				</div>
			)}
			<div className="wb-files-column" data-role="current">
				<FilesList
					rows={entries.map((entry) => ({ entry, depth: 0, expanded: false }))}
					cursor={state.cursor}
					marks={marks}
					facts={facts}
					onSelect={onSelect}
					onOpen={onOpen}
					scrollRef={scrollRef}
				/>
			</div>
			<div className="wb-files-column" data-role="preview">
				{focused?.directory ? (
					<FilesList
						rows={childEntries.map((entry) => ({
							entry,
							depth: 0,
							expanded: false,
						}))}
						cursor={null}
						marks={marks}
						facts={noFacts}
						onSelect={() => {}}
						onOpen={onOpen}
					/>
				) : focused ? (
					<FilesPreview entry={focused} preview={preview} {...facts(focused)} />
				) : (
					<p className="wb-files-note">{t("oqtoUi.files.empty")}</p>
				)}
			</div>
		</div>
	);
}
