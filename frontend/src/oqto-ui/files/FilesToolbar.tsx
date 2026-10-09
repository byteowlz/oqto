/**
 * The pane's toolbar: breadcrumb, sort keys, hidden entries, and the
 * details toggle. Every control dispatches a pure engine transition.
 */

import {
	ArrowDownAZ,
	ArrowDownWideNarrow,
	ChevronRight,
	Clock,
	Columns3,
	Eye,
	EyeOff,
	List,
	ListTree,
} from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { breadcrumb } from "./entries";
import { setSort } from "./navigation";
import type { FilesState } from "./navigator";
import type { FilesStore } from "./store";

interface FilesToolbarProps {
	readonly state: FilesState;
	readonly store: FilesStore;
	readonly view: { readonly details: boolean; readonly tree: boolean };
	readonly onToggle: (which: "details" | "tree") => void;
	/** The Container's own controls, placed at the toolbar's end. */
	readonly chrome?: ReactNode;
}

export function FilesToolbar({
	state,
	store,
	view,
	onToggle,
	chrome,
}: FilesToolbarProps) {
	const { t } = useTranslation();
	return (
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
			{(
				[
					["name", ArrowDownAZ, "oqtoUi.files.sortName"],
					["size", ArrowDownWideNarrow, "oqtoUi.files.sortSize"],
					["modified", Clock, "oqtoUi.files.sortModified"],
				] as const
			).map(([key, Icon, label]) => (
				<button
					key={key}
					className="wb-icon-button"
					type="button"
					data-active={state.sort.key === key || undefined}
					data-descending={
						(state.sort.key === key && state.sort.descending) || undefined
					}
					aria-label={t(label)}
					aria-pressed={state.sort.key === key}
					onClick={() => store.update((current) => setSort(current, key))}
				>
					<Icon aria-hidden="true" />
				</button>
			))}
			<button
				className="wb-icon-button"
				type="button"
				data-active={state.showHidden || undefined}
				aria-label={t(
					state.showHidden
						? "oqtoUi.files.hiddenHide"
						: "oqtoUi.files.hiddenShow",
				)}
				aria-pressed={state.showHidden}
				onClick={() =>
					store.update((current) => ({
						...current,
						showHidden: !current.showHidden,
					}))
				}
			>
				{state.showHidden ? (
					<Eye aria-hidden="true" />
				) : (
					<EyeOff aria-hidden="true" />
				)}
			</button>
			<button
				className="wb-icon-button"
				type="button"
				data-active={view.tree || undefined}
				aria-label={t(
					view.tree ? "oqtoUi.files.viewList" : "oqtoUi.files.viewTree",
				)}
				aria-pressed={view.tree}
				onClick={() => onToggle("tree")}
			>
				{view.tree ? (
					<List aria-hidden="true" />
				) : (
					<ListTree aria-hidden="true" />
				)}
			</button>
			<button
				className="wb-icon-button"
				type="button"
				data-active={view.details || undefined}
				aria-label={t("oqtoUi.files.details")}
				aria-pressed={view.details}
				onClick={() => onToggle("details")}
			>
				<Columns3 aria-hidden="true" />
			</button>
			{chrome}
		</div>
	);
}
