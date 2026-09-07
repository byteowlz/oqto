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

interface FileRowProps {
	readonly entry: FileEntry;
	/** Pre-formatted facts; empty strings hide the column. */
	readonly size: string;
	readonly modified: string;
	readonly cursor: boolean;
	readonly selected: boolean;
	readonly changed: boolean;
	readonly onSelect: (path: string, event: React.MouseEvent) => void;
	readonly onOpen: (entry: FileEntry) => void;
}

export const FileRow = memo(function FileRow({
	entry,
	size,
	modified,
	cursor,
	selected,
	changed,
	onSelect,
	onOpen,
}: FileRowProps) {
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
			onClick={(event) => onSelect(entry.path, event)}
			onDoubleClick={() => onOpen(entry)}
		>
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
