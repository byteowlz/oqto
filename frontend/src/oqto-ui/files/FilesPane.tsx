/**
 * The Files pane: a keyboard-first, virtualized directory browser. Only
 * the rows on screen are rendered, listings are fetched one level at a
 * time, and every interaction is a pure engine transition applied to the
 * store.
 *
 * Keys: j/k or arrows move, h or Backspace goes up, l or Enter opens,
 * g/G jump to the ends, "/" filters in place, Escape clears.
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronRight } from "lucide-react";
import {
	type KeyboardEvent,
	useCallback,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import type { FileEntry } from "../platform/files-contract";
import { FileRow } from "./FileRow";
import { breadcrumb } from "./entries";
import { cursorEntry, cursorToEdge, goUp } from "./navigation";
import { moveCursor, select, setFilter, visibleEntries } from "./navigator";
import type { FilesStore } from "./store";
import { useFilesState } from "./useFilesStore";

const ROW_HEIGHT = 26;

interface FilesPaneProps {
	readonly store: FilesStore;
	/** Opens a file in a host viewer; directories are handled internally. */
	readonly onOpenFile?: (entry: FileEntry) => void;
}

export function FilesPane({ store, onOpenFile }: FilesPaneProps) {
	const { t } = useTranslation();
	const state = useFilesState(store);
	const [filtering, setFiltering] = useState(false);
	const scrollRef = useRef<HTMLDivElement | null>(null);
	const entries = visibleEntries(state);
	const listing = state.listings[state.cwd];
	const selection = useMemo(() => new Set(state.selection), [state.selection]);
	const changed = useMemo(() => new Set(state.changed), [state.changed]);

	const virtualizer = useVirtualizer({
		count: entries.length,
		getScrollElement: () => scrollRef.current,
		estimateSize: () => ROW_HEIGHT,
		overscan: 12,
		// Render a screenful on the first frame instead of nothing, before
		// the host reports the real box.
		initialRect: { width: 320, height: 640 },
	});

	const open = useCallback(
		(entry: FileEntry) => {
			if (entry.directory) store.open(entry.path);
			else onOpenFile?.(entry);
		},
		[store, onOpenFile],
	);

	const onSelect = useCallback(
		(path: string, event: React.MouseEvent) => {
			const mode =
				event.metaKey || event.ctrlKey
					? "toggle"
					: event.shiftKey
						? "range"
						: "replace";
			store.update((current) => select(current, path, mode));
		},
		[store],
	);

	const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
		const key = event.key;
		const step = (delta: number) =>
			store.update((current) => moveCursor(current, delta));
		if (key === "ArrowDown" || key === "j") step(1);
		else if (key === "ArrowUp" || key === "k") step(-1);
		else if (key === "PageDown") step(10);
		else if (key === "PageUp") step(-10);
		else if (key === "Home" || key === "g")
			store.update((c) => cursorToEdge(c, "first"));
		else if (key === "End" || key === "G")
			store.update((c) => cursorToEdge(c, "last"));
		else if (key === "ArrowLeft" || key === "h" || key === "Backspace") {
			store.update(goUp);
		} else if (key === "ArrowRight" || key === "l" || key === "Enter") {
			const target = cursorEntry(state);
			if (target) open(target);
		} else if (key === "/") setFiltering(true);
		else if (key === "Escape") {
			setFiltering(false);
			store.update((current) => setFilter(current, ""));
		} else return;
		event.preventDefault();
	};

	const cursorIndex = entries.findIndex((entry) => entry.path === state.cursor);
	if (cursorIndex >= 0)
		virtualizer.scrollToIndex(cursorIndex, { align: "auto" });

	return (
		<aside className="wb-panel wb-files" aria-label={t("oqtoUi.files.label")}>
			<div className="wb-files-toolbar">
				<button
					className="wb-files-crumb"
					type="button"
					onClick={() => store.open("")}
				>
					{t("oqtoUi.files.rootLabel")}
				</button>
				{breadcrumb(state.cwd).map((crumb) => (
					<span className="wb-files-crumb-group" key={crumb.path}>
						<ChevronRight aria-hidden="true" />
						<button
							className="wb-files-crumb"
							type="button"
							onClick={() => store.open(crumb.path)}
						>
							{crumb.name}
						</button>
					</span>
				))}
			</div>

			{filtering || state.filter !== "" ? (
				<input
					className="wb-files-filter"
					type="search"
					value={state.filter}
					placeholder={t("oqtoUi.files.filter")}
					aria-label={t("oqtoUi.files.filter")}
					onChange={(event) =>
						store.update((current) => setFilter(current, event.target.value))
					}
					onKeyDown={(event) => {
						if (event.key === "Escape") {
							setFiltering(false);
							store.update((current) => setFilter(current, ""));
						}
					}}
				/>
			) : null}

			<div
				className="wb-files-rows"
				ref={scrollRef}
				// biome-ignore lint/a11y/noNoninteractiveTabindex: the listing is a keyboard-navigated grid.
				tabIndex={0}
				role="listbox"
				aria-label={t("oqtoUi.files.tree")}
				onKeyDown={onKeyDown}
			>
				{listing?.status === "error" ? (
					<p className="wb-files-note">{t("oqtoUi.files.failed")}</p>
				) : listing?.status !== "ready" ? (
					<p className="wb-files-note">{t("oqtoUi.files.loading")}</p>
				) : entries.length === 0 ? (
					<p className="wb-files-note">
						{state.filter === ""
							? t("oqtoUi.files.empty")
							: t("oqtoUi.files.noMatches")}
					</p>
				) : (
					<div
						className="wb-files-viewport"
						style={
							{
								"--wb-files-height": `${virtualizer.getTotalSize()}px`,
							} as React.CSSProperties
						}
					>
						{virtualizer.getVirtualItems().map((item) => {
							const entry = entries[item.index];
							return (
								<div
									className="wb-files-row"
									key={entry.path}
									style={
										{
											"--wb-files-offset": `${item.start}px`,
										} as React.CSSProperties
									}
								>
									<FileRow
										entry={entry}
										cursor={entry.path === state.cursor}
										selected={selection.has(entry.path)}
										changed={changed.has(entry.path)}
										onSelect={onSelect}
										onOpen={open}
									/>
								</div>
							);
						})}
					</div>
				)}
			</div>

			<footer className="wb-files-status">
				<span>{t("oqtoUi.files.entryCount", { count: entries.length })}</span>
				{state.selection.length > 0 ? (
					<span>
						{t("oqtoUi.files.selectedCount", { count: state.selection.length })}
					</span>
				) : null}
			</footer>
		</aside>
	);
}
