/**
 * One virtualized column of entries. Both the single-column pane and each
 * Miller column render through this, so the two fidelities share one row
 * implementation rather than forking the view.
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import { type CSSProperties, useRef } from "react";
import type { FileEntry } from "../platform/files-contract";
import { FileRow } from "./FileRow";

const ROW_HEIGHT = 26;

export interface EntryFacts {
	readonly size: string;
	readonly modified: string;
}

interface FilesListProps {
	readonly entries: readonly FileEntry[];
	readonly cursor: string | null;
	readonly selection: ReadonlySet<string>;
	readonly changed: ReadonlySet<string>;
	readonly facts: (entry: FileEntry) => EntryFacts;
	readonly onSelect: (path: string, event: React.MouseEvent) => void;
	readonly onOpen: (entry: FileEntry) => void;
	/** Scrolls this column to its cursor; the pane calls it after a move. */
	readonly scrollRef?: (scrollToCursor: () => void) => void;
}

export function FilesList({
	entries,
	cursor,
	selection,
	changed,
	facts,
	onSelect,
	onOpen,
	scrollRef,
}: FilesListProps) {
	const container = useRef<HTMLDivElement | null>(null);
	const virtualizer = useVirtualizer({
		count: entries.length,
		getScrollElement: () => container.current,
		estimateSize: () => ROW_HEIGHT,
		overscan: 12,
		initialRect: { width: 320, height: 640 },
	});
	scrollRef?.(() => {
		const index = entries.findIndex((entry) => entry.path === cursor);
		if (index >= 0) virtualizer.scrollToIndex(index, { align: "auto" });
	});
	return (
		<div className="wb-files-rows" ref={container}>
			<div
				className="wb-files-viewport"
				style={
					{
						"--wb-files-height": `${virtualizer.getTotalSize()}px`,
					} as CSSProperties
				}
			>
				{virtualizer.getVirtualItems().map((item) => {
					const entry = entries[item.index];
					return (
						<div
							className="wb-files-row"
							key={entry.path}
							style={
								{ "--wb-files-offset": `${item.start}px` } as CSSProperties
							}
						>
							<FileRow
								entry={entry}
								{...facts(entry)}
								cursor={entry.path === cursor}
								selected={selection.has(entry.path)}
								changed={changed.has(entry.path)}
								onSelect={onSelect}
								onOpen={onOpen}
							/>
						</div>
					);
				})}
			</div>
		</div>
	);
}
