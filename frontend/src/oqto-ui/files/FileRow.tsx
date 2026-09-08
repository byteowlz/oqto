/**
 * One directory row: icon, name, and optional facts. Memoized on its entry
 * and flags so scrolling a large directory never rerenders a row that did
 * not change. Colour comes from the entry kind's palette role, never from
 * a per-extension hex.
 */

import {
	Archive,
	Binary,
	Braces,
	FileCode2,
	FileImage,
	FileText,
	FileType2,
	Film,
	Folder,
	Link2,
	File as Plain,
} from "lucide-react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { memo } from "react";
import type { FileEntry } from "../platform/files-contract";
import { type EntryKind, entryKind } from "./kinds";

const ICONS: { readonly [kind in EntryKind]: typeof Folder } = {
	folder: Folder,
	code: FileCode2,
	markup: FileType2,
	data: Braces,
	document: FileText,
	image: FileImage,
	media: Film,
	archive: Archive,
	binary: Binary,
	file: Plain,
};

/** How a row appears: its facts, its flags, and its tree placement. */
export interface RowState {
	/** Pre-formatted facts; empty strings hide the column. */
	readonly size: string;
	readonly modified: string;
	readonly cursor: boolean;
	readonly selected: boolean;
	readonly changed: boolean;
	/** Tree view only: nesting level and expansion, null in list view. */
	readonly tree: { readonly depth: number; readonly expanded: boolean } | null;
}

interface FileRowProps {
	readonly entry: FileEntry;
	readonly row: RowState;
	readonly onSelect: (path: string, event: React.MouseEvent) => void;
	readonly onOpen: (entry: FileEntry) => void;
	readonly onToggle: (entry: FileEntry) => void;
}

export const FileRow = memo(function FileRow({
	entry,
	row,
	onSelect,
	onOpen,
	onToggle,
}: FileRowProps) {
	const { size, modified, cursor, selected, changed, tree } = row;
	const kind = entryKind(entry.name, entry.directory);
	const Icon = ICONS[kind];
	return (
		<button
			type="button"
			className="wb-tree__row"
			data-cursor={cursor || undefined}
			data-selected={selected || undefined}
			data-changed={changed || undefined}
			data-kind={kind}
			data-depth={tree ? tree.depth : undefined}
			onClick={(event) => onSelect(entry.path, event)}
			onDoubleClick={() => onOpen(entry)}
		>
			{tree ? (
				entry.directory ? (
					<span
						className="wb-tree__expander"
						onPointerDown={(event) => {
							event.stopPropagation();
							event.preventDefault();
							onToggle(entry);
						}}
					>
						{tree.expanded ? (
							<ChevronDown aria-hidden="true" />
						) : (
							<ChevronRight aria-hidden="true" />
						)}
					</span>
				) : (
					<span className="wb-tree__expander" />
				)
			) : null}
			<Icon aria-hidden="true" />
			<span className="wb-tree__name">{entry.name}</span>
			{entry.symlink ? (
				<Link2 className="wb-tree__link" aria-hidden="true" />
			) : null}
			{modified === "" ? null : (
				<span className="wb-tree__meta">{modified}</span>
			)}
			{size === "" ? null : <span className="wb-tree__meta">{size}</span>}
		</button>
	);
});
