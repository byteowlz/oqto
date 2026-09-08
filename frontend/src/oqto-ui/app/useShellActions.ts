/**
 * The shell's own affordances as semantic transactions: opening interface
 * settings, opening a file as Content, collapsing the navigation, toggling
 * the side Container, and the catalogue the add control offers. Kept out of
 * the shell component so that file stays a composition, not a controller.
 */

import { useCallback, useMemo } from "react";
import type { PersistedCompositorStore } from "../compositor/react/persisted-store";
import {
	GALLERY_CONTENT,
	SESSIONS_CONTENT,
	SETTINGS_CONTENT,
	TODOS_CONTENT,
	chatContent,
	fileContent,
	filesContent,
} from "./compositorRefs";

interface ShellActionsInput {
	readonly store: PersistedCompositorStore;
	readonly settingsOpen: boolean;
	/** Below the mobile breakpoint the navigation is a drawer, not a Container. */
	readonly mobile: boolean;
	readonly closeDrawer: () => void;
	readonly sessionId: string;
	readonly workDirectoryId: string;
}

export function useShellActions(input: ShellActionsInput) {
	const { store, settingsOpen, mobile, closeDrawer } = input;

	const closeSettings = useCallback(() => {
		store.commit([{ type: "close", contentId: SETTINGS_CONTENT.id }]);
	}, [store]);

	const toggleSettings = useCallback(() => {
		if (settingsOpen) closeSettings();
		else
			store.commit([
				{
					type: "open",
					content: SETTINGS_CONTENT,
					target: { role: "auxiliary" },
				},
			]);
	}, [settingsOpen, closeSettings, store]);

	const chrome = useMemo(() => {
		const toggleRole = (role: string, collapsed?: boolean) => {
			const container = store
				.getSnapshot()
				.containers.find((candidate) => candidate.role === role);
			if (!container) return;
			store.commit([
				{
					type: "collapse",
					containerId: container.id,
					collapsed: collapsed ?? !container.collapsed,
				},
			]);
		};
		return {
			openFile: (path: string) => {
				store.commit([
					{
						type: "open",
						content: fileContent(path),
						target: { role: "primary" },
					},
				]);
			},
			collapseNavigation: () => {
				// On mobile the navigation is a drawer, so dismissing it is drawer
				// state. Committing a layout collapse there would follow the user
				// back to the desktop and hide the sidebar behind its expand rail.
				if (mobile) {
					closeDrawer();
					return;
				}
				toggleRole("navigation", true);
			},
			toggleAuxiliary: () => toggleRole("auxiliary"),
		};
	}, [store, mobile, closeDrawer]);

	const settings = useMemo(
		() => ({
			open: settingsOpen,
			toggle: toggleSettings,
			close: closeSettings,
		}),
		[settingsOpen, toggleSettings, closeSettings],
	);

	const addable = useMemo(
		() => [
			chatContent(input.sessionId),
			filesContent(input.workDirectoryId),
			TODOS_CONTENT,
			GALLERY_CONTENT,
			SESSIONS_CONTENT,
			SETTINGS_CONTENT,
		],
		[input.sessionId, input.workDirectoryId],
	);

	return { settings, chrome, addable };
}
