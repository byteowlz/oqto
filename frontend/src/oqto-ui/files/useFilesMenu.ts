/**
 * The Files resource menu's state: what it is open on, and the Actions the
 * Broker offered for those subjects. Opening asks the Broker once, so the
 * menu shows contributed Actions without the pane knowing what they are.
 */

import { useCallback, useRef, useState } from "react";
import type {
	ActionHost,
	ActionOffer,
	ResourceSubject,
} from "../platform/actions-contract";
import type { MenuMode } from "./menu";

/** Viewport point the menu opens at. */
export interface MenuPoint {
	readonly x: number;
	readonly y: number;
}

export interface MenuAnchor extends MenuPoint {
	readonly mode: MenuMode;
	/** Entry the menu was opened on; null for the directory itself. */
	readonly targetPath: string | null;
	readonly subjects: readonly ResourceSubject[];
	readonly offers: readonly ActionOffer[];
}

export interface FilesMenuState {
	readonly anchor: MenuAnchor | null;
	open(
		at: MenuPoint,
		targetPath: string | null,
		subjects: readonly ResourceSubject[],
	): void;
	close(): void;
	/** Switches the open menu to picking a copy destination. */
	showDestinations(): void;
	/** Runs a contributed Action on the subjects the menu was opened for. */
	run(actionId: string): void;
}

export function useFilesMenu(actionHost: ActionHost): FilesMenuState {
	const [anchor, setAnchor] = useState<MenuAnchor | null>(null);
	const opened = useRef(0);

	const open = useCallback(
		(
			at: MenuPoint,
			targetPath: string | null,
			subjects: readonly ResourceSubject[],
		) => {
			opened.current += 1;
			const generation = opened.current;
			setAnchor({ ...at, mode: "commands", targetPath, subjects, offers: [] });
			actionHost.offers("resource-menu", subjects).then(
				(offers) => {
					// A menu opened since this request must not be overwritten.
					if (opened.current !== generation) return;
					setAnchor((current) =>
						current === null ? current : { ...current, offers },
					);
				},
				() => {},
			);
		},
		[actionHost],
	);

	const close = useCallback(() => {
		opened.current += 1;
		setAnchor(null);
	}, []);

	const showDestinations = useCallback(() => {
		setAnchor((current) =>
			current === null ? current : { ...current, mode: "destinations" },
		);
	}, []);

	const run = useCallback(
		(actionId: string) => {
			const subjects = anchor?.subjects ?? [];
			close();
			void actionHost.invoke(actionId, subjects);
		},
		[actionHost, anchor, close],
	);

	return { anchor, open, close, showDestinations, run };
}

/**
 * Keeps the listing's focus across a menu: the element the menu was opened
 * from takes focus back when it closes, so the keyboard never lands nowhere.
 */
export function useMenuFocus(menu: FilesMenuState) {
	const ref = useRef<HTMLDivElement | null>(null);
	return {
		ref,
		close: () => {
			menu.close();
			ref.current?.focus();
		},
	};
}
