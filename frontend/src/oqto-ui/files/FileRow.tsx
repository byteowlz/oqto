/**
 * One directory row. Memoized on its entry and flags so scrolling a large
 * directory never rerenders rows that did not change.
 */

import { File, Folder, Link2 } from "lucide-react";
import { memo } from "react";
import type { FileEntry } from "../platform/files-contract";

interface FileRowProps {
	readonly entry: FileEntry;
	readonly cursor: boolean;
	readonly selected: boolean;
	readonly changed: boolean;
	readonly onSelect: (path: string, event: React.MouseEvent) => void;
	readonly onOpen: (entry: FileEntry) => void;
}

export const FileRow = memo(function FileRow({
	entry,
	cursor,
	selected,
	changed,
	onSelect,
	onOpen,
}: FileRowProps) {
	return (
		<button
			type="button"
			className="wb-tree__row"
			data-cursor={cursor || undefined}
			data-selected={selected || undefined}
			data-changed={changed || undefined}
			data-kind={entry.directory ? "folder" : "file"}
			onClick={(event) => onSelect(entry.path, event)}
			onDoubleClick={() => onOpen(entry)}
		>
			{entry.directory ? (
				<Folder aria-hidden="true" />
			) : entry.symlink ? (
				<Link2 aria-hidden="true" />
			) : (
				<File aria-hidden="true" />
			)}
			<span className="wb-tree__name">{entry.name}</span>
		</button>
	);
});
