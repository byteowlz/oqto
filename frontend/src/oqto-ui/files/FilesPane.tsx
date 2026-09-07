/**
 * The Files pane: a keyboard-first, virtualized directory browser. Only
 * the rows on screen are rendered, listings are fetched one level at a
 * time, and every interaction is a pure engine transition applied to the
 * store.
 *
 * Keys: j/k or arrows move, h or Backspace goes up, l or Enter opens,
 * g/G jump to the ends, "/" filters in place, Escape clears.
 */

import { ChevronRight, Columns3 } from "lucide-react";
import {
	type KeyboardEvent,
	useCallback,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import type { FileEntry } from "../platform/files-contract";
import { FilesList } from "./FilesList";
import { FilesPreview } from "./FilesPreview";
import { type ColumnRendering, MillerColumns } from "./MillerColumns";
import { breadcrumb } from "./entries";
import { paneFidelity, showsColumns, showsFacts } from "./fidelity";
import { formatModified, formatSize } from "./format";
import { cursorEntry, cursorToEdge, goUp } from "./navigation";
import { moveCursor, select, visibleEntries } from "./navigator";
import type { FilesStore } from "./store";
import { useElementSize } from "./useElementSize";
import { useFileActions } from "./useFileActions";
import { useFilesState } from "./useFilesStore";
import { usePreview } from "./usePreview";

const ROW_HEIGHT = 26;

interface FilesPaneProps {
	readonly store: FilesStore;
	/** Opens a file in a host viewer; directories are handled internally. */
	readonly onOpenFile?: (entry: FileEntry) => void;
}

export function FilesPane({ store, onOpenFile }: FilesPaneProps) {
	const { t, i18n } = useTranslation();
	const state = useFilesState(store);
	const [details, setDetails] = useState(true);
	const [previewOpen, setPreviewOpen] = useState(false);
	const actions = useFileActions(store);
	const measured = useElementSize();
	const fidelity = paneFidelity(measured.inlineSize);
	const columns = showsColumns(fidelity);
	const entries = visibleEntries(state);
	const listing = state.listings[state.cwd];
	const focused = entries.find((entry) => entry.path === state.cursor) ?? null;
	if (columns) {
		const parent =
			state.cwd === "" ? null : state.cwd.slice(0, state.cwd.lastIndexOf("/"));
		if (parent !== null) store.load(parent);
		if (focused?.directory) store.load(focused.path);
	}
	const preview = usePreview(
		store.context.fileSystem,
		store.context.workspacePath,
	);
	const selection = useMemo(() => new Set(state.selection), [state.selection]);
	const changed = useMemo(() => new Set(state.changed), [state.changed]);

	const facts_ = details && showsFacts(fidelity);
	const facts = useMemo(() => {
		const labels = {
			bytes: (count: number) => t("oqtoUi.files.sizeBytes", { count }),
			kilo: (value: string) => t("oqtoUi.files.sizeKilo", { value }),
			mega: (value: string) => t("oqtoUi.files.sizeMega", { value }),
			giga: (value: string) => t("oqtoUi.files.sizeGiga", { value }),
		};
		const now = Date.now();
		return (entry: FileEntry) => ({
			size: !facts_ || entry.directory ? "" : formatSize(entry.size, labels),
			modified: facts_
				? formatModified(entry.modifiedAt, i18n.language, now)
				: "",
		});
	}, [facts_, t, i18n.language]);

	const scrollToCursor = useRef<() => void>(() => {});
	const holdScroll = useCallback((scroll: () => void) => {
		scrollToCursor.current = scroll;
	}, []);

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
			if (previewOpen) {
				const snapshot = store.getSnapshot();
				preview.show(
					visibleEntries(snapshot).find((entry) => entry.path === path) ?? null,
				);
			}
		},
		[store, previewOpen, preview],
	);

	const rendering: ColumnRendering = {
		selection,
		changed,
		facts,
		onSelect,
		onOpen: open,
		scrollRef: holdScroll,
	};

	/** Keeps the scroll position and Quick Look in step after a cursor move. */
	const settleCursor = () => {
		const snapshot = store.getSnapshot();
		scrollToCursor.current();
		const next =
			visibleEntries(snapshot).find(
				(entry) => entry.path === snapshot.cursor,
			) ?? null;
		if (previewOpen || columns) preview.show(next);
	};

	const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
		const key = event.key;
		const step = (delta: number) => {
			store.update((current) => moveCursor(current, delta));
			settleCursor();
		};
		if (key === "ArrowDown" || key === "j") step(1);
		else if (key === "ArrowUp" || key === "k") step(-1);
		else if (key === "PageDown") step(10);
		else if (key === "PageUp") step(-10);
		else if (key === "Home" || key === "g") {
			store.update((c) => cursorToEdge(c, "first"));
			settleCursor();
		} else if (key === "End" || key === "G") {
			store.update((c) => cursorToEdge(c, "last"));
			settleCursor();
		} else if (key === "ArrowLeft" || key === "h" || key === "Backspace") {
			store.update(goUp);
		} else if (key === "ArrowRight" || key === "l" || key === "Enter") {
			const target = cursorEntry(state);
			if (target) open(target);
		} else if (key === "/") actions.begin("filter");
		else if (key === " ") {
			const opening = !previewOpen;
			setPreviewOpen(opening);
			preview.show(opening ? focused : null);
		} else if (key === "r" || key === "F2") actions.begin("rename");
		else if (key === "n" && (event.ctrlKey || event.metaKey))
			actions.begin("create");
		else if (key === "Delete") actions.begin("confirmDelete");
		else if (key === "u") actions.undo();
		else if (key === "Escape") {
			setPreviewOpen(false);
			actions.cancel();
		} else return;
		event.preventDefault();
	};

	return (
		<aside
			className="wb-panel wb-files"
			data-fidelity={fidelity}
			ref={measured.ref}
			aria-label={t("oqtoUi.files.label")}
		>
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
				<span className="wb-files-toolbar__spacer" />
				<button
					className="wb-icon-button"
					type="button"
					data-active={details || undefined}
					aria-label={t("oqtoUi.files.details")}
					aria-pressed={details}
					onClick={() => setDetails((shown) => !shown)}
				>
					<Columns3 aria-hidden="true" />
				</button>
			</div>

			{actions.mode === null ? null : actions.mode === "confirmDelete" ? (
				<div className="wb-files-filter wb-files-confirm">
					<span>
						{t("oqtoUi.files.deleteConfirm", { name: focused?.name ?? "" })}
					</span>
					<button type="button" onClick={actions.submit}>
						{t("oqtoUi.files.delete")}
					</button>
					<button type="button" onClick={actions.cancel}>
						{t("common.cancel")}
					</button>
				</div>
			) : (
				// biome-ignore lint/a11y/noAutofocus: the action line is opened by a key and must receive it.
				<input
					className="wb-files-filter"
					type="text"
					autoFocus
					value={actions.draft}
					placeholder={t(
						actions.mode === "rename"
							? "oqtoUi.files.rename"
							: actions.mode === "create"
								? "oqtoUi.files.newFolderPrompt"
								: "oqtoUi.files.filter",
					)}
					aria-label={t(
						actions.mode === "rename"
							? "oqtoUi.files.rename"
							: actions.mode === "create"
								? "oqtoUi.files.newFolderPrompt"
								: "oqtoUi.files.filter",
					)}
					onChange={(event) => actions.change(event.target.value)}
					onKeyDown={(event) => {
						if (event.key === "Enter") {
							event.preventDefault();
							actions.submit();
						} else if (event.key === "Escape") {
							event.preventDefault();
							actions.cancel();
						}
					}}
				/>
			)}

			<div
				className="wb-files-body"
				data-fidelity={fidelity}
				// biome-ignore lint/a11y/noNoninteractiveTabindex: the listing is keyboard-navigated.
				tabIndex={0}
				role="listbox"
				aria-label={t("oqtoUi.files.tree")}
				onKeyDown={onKeyDown}
			>
				{listing?.status === "error" ? (
					<p className="wb-files-note">{t("oqtoUi.files.failed")}</p>
				) : listing?.status !== "ready" ? (
					<p className="wb-files-note">{t("oqtoUi.files.loading")}</p>
				) : entries.length === 0 && !columns ? (
					<p className="wb-files-note">
						{state.filter === ""
							? t("oqtoUi.files.empty")
							: t("oqtoUi.files.noMatches")}
					</p>
				) : columns ? (
					<MillerColumns
						state={state}
						entries={entries}
						focused={focused}
						preview={preview.state}
						rendering={rendering}
					/>
				) : (
					<FilesList
						entries={entries}
						cursor={state.cursor}
						selection={selection}
						changed={changed}
						facts={facts}
						onSelect={onSelect}
						onOpen={open}
						scrollRef={holdScroll}
					/>
				)}
			</div>

			{!columns && previewOpen && focused ? (
				<FilesPreview
					entry={focused}
					preview={preview.state}
					{...facts(focused)}
				/>
			) : null}

			<footer className="wb-files-status">
				<span>{t("oqtoUi.files.entryCount", { count: entries.length })}</span>
				{state.selection.length > 0 ? (
					<span>
						{t("oqtoUi.files.selectedCount", { count: state.selection.length })}
					</span>
				) : null}
				<span className="wb-files-status__spacer" />
				{actions.outcome ? (
					<span>
						{t(`oqtoUi.files.${actions.outcome.key}`, {
							name: actions.outcome.name,
							message: actions.outcome.name,
						})}
					</span>
				) : null}
				{actions.canUndo ? (
					<button type="button" onClick={actions.undo}>
						{t("oqtoUi.files.undo")}
					</button>
				) : null}
			</footer>
		</aside>
	);
}
