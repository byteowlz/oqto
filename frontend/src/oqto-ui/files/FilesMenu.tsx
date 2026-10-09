/**
 * The Files resource menu: a host-owned surface (ADR-0045) projecting the
 * pane's own commands and whatever Actions the Broker says are eligible for
 * the subjects under the pointer. It runs nothing itself — every choice is
 * handed back to the host.
 */

import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import {
	type CopyDestination,
	type MenuGroup,
	type MenuItem,
	fileMenu,
} from "./menu";
import type { FilesState } from "./navigator";
import type { FilesMenuState } from "./useFilesMenu";

/** Running one menu item: it carries whatever it references. */
export type MenuRun = (item: MenuItem) => void;

interface SurfaceProps {
	readonly menu: FilesMenuState;
	readonly state: FilesState;
	/** Other work directories the selection can be copied into. */
	readonly destinations: readonly CopyDestination[];
	readonly onRun: MenuRun;
	readonly onClose: () => void;
}

/** Renders the menu when one is open, on the entry it was opened for. */
export function FilesMenuSurface({
	menu,
	state,
	destinations,
	onRun,
	onClose,
}: SurfaceProps) {
	const anchor = menu.anchor;
	if (!anchor) return null;
	return (
		<FilesMenu
			groups={fileMenu(
				state,
				anchor.targetPath,
				anchor.offers,
				destinations,
				anchor.mode,
			)}
			at={anchor}
			onRun={onRun}
			onClose={onClose}
		/>
	);
}

interface FilesMenuProps {
	readonly groups: readonly MenuGroup[];
	readonly at: { readonly x: number; readonly y: number };
	readonly onRun: MenuRun;
	readonly onClose: () => void;
}

function FilesMenu({ groups, at, onRun, onClose }: FilesMenuProps) {
	const { t } = useTranslation();
	return (
		<>
			{/* Clicking anywhere else dismisses the menu. */}
			<button
				type="button"
				className="wb-files-menu__backdrop"
				aria-label={t("common.cancel")}
				onClick={onClose}
				onContextMenu={(event) => {
					event.preventDefault();
					onClose();
				}}
			/>
			<div
				className="wb-files-menu"
				role="menu"
				aria-label={t("oqtoUi.files.menu")}
				style={
					{
						"--wb-menu-x": `${at.x}px`,
						"--wb-menu-y": `${at.y}px`,
					} as CSSProperties
				}
				onKeyDown={(event) => {
					if (event.key === "Escape") onClose();
				}}
			>
				{groups.map((group) => (
					<div className="wb-files-menu__group" key={group.id}>
						{group.items.map((item) => (
							<button
								key={
									item.actionId ?? item.destination?.path ?? item.command ?? ""
								}
								type="button"
								role="menuitem"
								data-danger={item.danger || undefined}
								onClick={() => onRun(item)}
							>
								{item.labelKey
									? t(item.labelKey, { name: item.destination?.name ?? "" })
									: item.title}
							</button>
						))}
					</div>
				))}
			</div>
		</>
	);
}
