/**
 * Projects solved geometry onto CSS Grid template strings. Renderer output
 * derived from the kernel's solved tracks; nothing here is persisted and no
 * observed DOM geometry flows back into layout state.
 */

import type { GridPlacement } from "../index";

/** grid-template-columns/rows from solved logical lengths (px). */
export function gridTemplateFromSizes(sizes: readonly number[]): string {
	return sizes.length === 0
		? "none"
		: sizes.map((size) => `${size}px`).join(" ");
}

/** grid-row / grid-column line syntax for a placement, offset by `rowOffset`. */
export function gridLines(
	placement: GridPlacement,
	rowOffset: number,
): { readonly row: string; readonly column: string } {
	return {
		row: `${placement.row - rowOffset + 1} / span ${placement.rowSpan}`,
		column: `${placement.column + 1} / span ${placement.colSpan}`,
	};
}
