/**
 * The Files pane: a keyboard-first, virtualized directory browser. Only
 * the rows on screen are rendered, listings are fetched one level at a
 * time, and every interaction is a pure engine transition applied to the
 * store.
 *
 * Keys: j/k or arrows move, h or Backspace goes up, l or Enter opens,
 * g/G jump to the ends, "/" filters in place, Escape clears.
 */

import {
	type KeyboardEvent,
	type MouseEvent,
	type ReactNode,
	useCallback,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import type { ActionHost } from "../platform/actions-contract";
import type { FileEntry } from "../platform/files-contract";
import { FilesActionLine } from "./FilesActionLine";
import { FilesList } from "./FilesList";
import { FilesMenuSurface } from "./FilesMenu";
import { FilesPreview } from "./FilesPreview";
import { FilesToolbar } from "./FilesToolbar";
import { type ColumnRendering, MillerColumns } from "./MillerColumns";
import { yank } from "./clipboard";
import { paneFidelity, showsColumns, showsFacts } from "./fidelity";
import { formatModified, formatSize } from "./format";
import type { CopyDestination } from "./menu";

import { cursorEntry, cursorToEdge, goUp } from "./navigation";
import { moveCursor, placeCursor, select, visibleEntries } from "./navigator";
import type { FilesStore } from "./store";
import { toggleExpanded, treeRows } from "./tree";
import { useElementSize } from "./useElementSize";
import { useFileActions } from "./useFileActions";
import { useFilesMenu, useMenuFocus } from "./useFilesMenu";
import { useFilesState } from "./useFilesStore";
import { useMenuCommands } from "./useMenuCommands";
import { usePreview } from "./usePreview";

const ROW_HEIGHT = 26;

interface FilesPaneProps {
	readonly store: FilesStore;
	/** Opens a file in a host viewer; directories are handled internally. */
	readonly onOpenFile?: (entry: FileEntry) => void;
	/** The Container's own controls, placed in the toolbar. */
	readonly chrome?: ReactNode;
	/** Projects eligible Actions into the resource menu (ADR-0045). */
	readonly actionHost: ActionHost;
	/** Other work directories the selection can be copied into. */
	readonly destinations: readonly CopyDestination[];
}

export function FilesPane({
	store,
	onOpenFile,
	chrome,
	actionHost,
	destinations,
}: FilesPaneProps) {
	const { t, i18n } = useTranslation();
	const state = useFilesState(store);
	const [details, setDetails] = useState(true);
	const [previewOpen, setPreviewOpen] = useState(false);
	const [tree, setTree] = useState(false);
	const actions = useFileActions(store);
	const measured = useElementSize();
	const fidelity = paneFidelity(measured.inlineSize);
	const columns = showsColumns(fidelity) && !tree;
	const entries = visibleEntries(state);
	const listing = state.listings[state.cwd];
	const rows = tree
		? treeRows(state)
		: entries.map((entry) => ({ entry, depth: 0, expanded: false }));
	const focused = entries.find((entry) => entry.path === state.cursor) ?? null;
	if (columns) {
		const parent =
			state.cwd === "" ? null : state.cwd.slice(0, state.cwd.lastIndexOf("/"));
		if (parent !== null) store.load(parent);
		if (focused?.directory) store.load(focused.path);
	}
	const preview = usePreview(
		store.context.fileHost,
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

	const toggleTree = useCallback(
		(entry: FileEntry) => {
			let load: string | null = null;
			store.update((current) => {
				const result = toggleExpanded(current, entry.path);
				load = result.load;
				return result.state;
			});
			if (load) store.load(load);
		},
		[store],
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

	const marks = { selection, changed };
	const rendering: ColumnRendering = {
		marks,
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

	const menu = useFilesMenu(actionHost);
	const commands = useMenuCommands({ store, actions, menu, open });
	const focus = useMenuFocus(menu);

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
			if (!target) return;
			if (tree && target.directory && key !== "Enter") toggleTree(target);
			else open(target);
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
		else if (key === "y") store.update((current) => yank(current, "copy"));
		else if (key === "x") store.update((current) => yank(current, "move"));
		else if (key === "p") actions.transfer.paste();
		else if (key === "m" || key === "ContextMenu")
			commands.openAtCursor(event.currentTarget);
		else if (key === "Escape") {
			setPreviewOpen(false);
			menu.close();
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
			<FilesToolbar
				state={state}
				store={store}
				chrome={chrome}
				view={{ details, tree }}
				onToggle={(which) =>
					which === "details"
						? setDetails((shown) => !shown)
						: setTree((shown) => !shown)
				}
			/>

			<FilesActionLine actions={actions} target={focused?.name ?? ""} />
			<FilesMenuSurface
				menu={menu}
				state={state}
				destinations={destinations}
				onRun={commands.run}
				onClose={focus.close}
			/>

			<div
				className="wb-files-body"
				ref={focus.ref}
				data-fidelity={fidelity}
				// biome-ignore lint/a11y/noNoninteractiveTabindex: the listing is keyboard-navigated.
				tabIndex={0}
				role="listbox"
				aria-label={t("oqtoUi.files.tree")}
				onKeyDown={onKeyDown}
				onContextMenu={(event) => {
					const row = (event.target as HTMLElement).closest<HTMLElement>(
						"[data-path]",
					);
					commands.openMenu(event, row?.dataset.path ?? null);
				}}
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
						workspacePath={store.context.workspacePath}
						entries={entries}
						focused={focused}
						preview={preview.state}
						rendering={rendering}
					/>
				) : (
					<FilesList
						rows={rows}
						cursor={state.cursor}
						marks={marks}
						facts={facts}
						onSelect={onSelect}
						onOpen={open}
						onToggle={tree ? toggleTree : undefined}
						scrollRef={holdScroll}
					/>
				)}
			</div>

			{!columns && previewOpen && focused ? (
				<FilesPreview
					entry={focused}
					preview={preview.state}
					workspacePath={store.context.workspacePath}
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
				{state.clipboard ? (
					<span>
						{t(
							state.clipboard.mode === "copy"
								? "oqtoUi.files.yankCopy"
								: "oqtoUi.files.yankMove",
							{ count: state.clipboard.paths.length },
						)}
					</span>
				) : null}
				<span className="wb-files-status__spacer" />
				{actions.outcome ? (
					<span>
						{t(`oqtoUi.files.${actions.outcome.key}`, {
							name: actions.outcome.name ?? "",
							message: actions.outcome.name ?? "",
							count: actions.outcome.count ?? 0,
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
