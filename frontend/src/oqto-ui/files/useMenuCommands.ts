/**
 * Turning a resource-menu choice into work: host commands run through the
 * pane's own engine, contributed Actions go back to the Broker. Opening the
 * menu also moves the cursor, so a right-click acts on what it points at.
 */

import type { MouseEvent } from "react";
import { yank } from "./clipboard";
import type { FileEntry } from "./entries";
import type { MenuItem } from "./menu";
import { menuSubjects } from "./menu";
import { cursorEntry } from "./navigation";
import { placeCursor } from "./navigator";
import type { FilesStore } from "./store";
import type { FileActions } from "./useFileActions";
import type { FilesMenuState } from "./useFilesMenu";

interface MenuCommandsInput {
	readonly store: FilesStore;
	readonly actions: FileActions;
	readonly menu: FilesMenuState;
	/** Opens an entry the way the listing does: navigate or hand to the host. */
	readonly open: (entry: FileEntry) => void;
}

export interface MenuCommands {
	openMenu(event: MouseEvent<HTMLElement>, targetPath: string | null): void;
	/** Opens the menu from the keyboard, anchored to the cursor's own row. */
	openAtCursor(surface: HTMLElement): void;
	run(item: MenuItem): void;
}

export function useMenuCommands(input: MenuCommandsInput): MenuCommands {
	const { store, actions, menu, open } = input;
	return {
		openMenu(event, targetPath) {
			event.preventDefault();
			const state = store.getSnapshot();
			// A right-click outside the selection acts on what it points at.
			if (targetPath !== null && !state.selection.includes(targetPath)) {
				store.update((current) => placeCursor(current, targetPath));
			}
			menu.open(
				{ x: event.clientX, y: event.clientY },
				targetPath,
				menuSubjects(
					store.getSnapshot(),
					store.context.workspacePath,
					targetPath,
				),
			);
		},
		openAtCursor(surface) {
			const state = store.getSnapshot();
			const target = state.cursor;
			if (target === null) return;
			const row = surface.querySelector<HTMLElement>(
				`[data-path="${CSS.escape(target)}"]`,
			);
			const box = (row ?? surface).getBoundingClientRect();
			menu.open(
				{ x: Math.round(box.left + 24), y: Math.round(box.bottom) },
				target,
				menuSubjects(state, store.context.workspacePath, target),
			);
		},
		run(item) {
			if (item.actionId !== null) {
				menu.run(item.actionId);
				return;
			}
			const command = item.command;
			if (command === "copyTo" && !item.destination) {
				menu.showDestinations();
				return;
			}
			menu.close();
			if (command === "copyTo" && item.destination) {
				actions.transfer.copyTo(item.destination);
				return;
			}
			if (command === "open") {
				const target = cursorEntry(store.getSnapshot());
				if (target) open(target);
			} else if (command === "rename") actions.begin("rename");
			else if (command === "createFolder") actions.begin("create");
			else if (command === "delete") actions.begin("confirmDelete");
			else if (command === "copy")
				store.update((current) => yank(current, "copy"));
			else if (command === "cut")
				store.update((current) => yank(current, "move"));
			else if (command === "paste") actions.transfer.paste();
		},
	};
}
