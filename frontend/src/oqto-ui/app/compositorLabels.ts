/**
 * Translated compositor chrome labels, built once per language at the
 * composition edge from the stable action ids.
 */

import type { TFunction } from "i18next";
import type { CompositorChromeLabels } from "../compositor/react/contracts";
import {
	DEFAULT_KEY_BINDINGS,
	actionId,
} from "../compositor/react/keybindings";

/** Action id -> i18n key suffix under oqtoUi.compositor.actions. */
const ACTION_LABEL_KEYS: { [id: string]: string } = {
	"compositor.toggleNavigation": "toggleNavigation",
	"compositor.focus.start": "focusStart",
	"compositor.focus.end": "focusEnd",
	"compositor.focus.up": "focusUp",
	"compositor.focus.down": "focusDown",
	"compositor.move.start": "moveStart",
	"compositor.move.end": "moveEnd",
	"compositor.move.up": "moveUp",
	"compositor.move.down": "moveDown",
	"compositor.shrink": "shrink",
	"compositor.grow": "grow",
	"compositor.prevTab": "prevTab",
	"compositor.nextTab": "nextTab",
	"compositor.closeFocused": "closeFocused",
	"compositor.scrollStart": "scrollStart",
	"compositor.scrollEnd": "scrollEnd",
	"compositor.undo": "undo",
	"shell.openCommandPalette": "openPalette",
};

export function createChromeLabels(t: TFunction): CompositorChromeLabels {
	const actions: { [id: string]: string } = {};
	for (const binding of DEFAULT_KEY_BINDINGS) {
		const id = actionId(binding.action);
		actions[id] = t(`oqtoUi.compositor.actions.${ACTION_LABEL_KEYS[id] ?? id}`);
	}
	return {
		closeTab: t("common.close"),
		closeContainer: t("oqtoUi.compositor.closeContainer"),
		resizeColumns: t("oqtoUi.compositor.resizeColumns"),
		resizeRows: t("oqtoUi.compositor.resizeRows"),
		dropTop: t("oqtoUi.compositor.dropTop"),
		dropBottom: t("oqtoUi.compositor.dropBottom"),
		dropStart: t("oqtoUi.compositor.dropStart"),
		dropEnd: t("oqtoUi.compositor.dropEnd"),
		expandStart: t("oqtoUi.compositor.expandStart"),
		add: {
			open: t("oqtoUi.compositor.add.open"),
			placements: {
				tab: t("oqtoUi.compositor.add.tab"),
				"inline-start": t("oqtoUi.compositor.add.inlineStart"),
				"inline-end": t("oqtoUi.compositor.add.inlineEnd"),
				"block-start": t("oqtoUi.compositor.add.blockStart"),
				"block-end": t("oqtoUi.compositor.add.blockEnd"),
			},
		},
		palette: {
			title: t("oqtoUi.compositor.palette.title"),
			searchPlaceholder: t("oqtoUi.compositor.palette.search"),
			noMatches: t("oqtoUi.compositor.palette.noMatches"),
			actions,
		},
	};
}
