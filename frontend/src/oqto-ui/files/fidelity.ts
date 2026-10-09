/**
 * Files fidelity, derived from the container the pane was allocated
 * (ADR-0037): one View model, progressively disclosed. Narrow shows a
 * single dense column; wide shows Miller columns with the parent, the
 * current directory, and a preview.
 */

export type FilesFidelity = "compact" | "standard" | "expanded";

/** Widths are logical pixels of the pane itself, not the viewport. */
const STANDARD_FROM = 380;
const EXPANDED_FROM = 760;

export function paneFidelity(inlineSize: number): FilesFidelity {
	if (inlineSize >= EXPANDED_FROM) return "expanded";
	if (inlineSize >= STANDARD_FROM) return "standard";
	return "compact";
}

/** Size and date columns need room; the narrowest mode drops them. */
export function showsFacts(fidelity: FilesFidelity): boolean {
	return fidelity !== "compact";
}

/** Miller columns only when the pane is wide enough to earn them. */
export function showsColumns(fidelity: FilesFidelity): boolean {
	return fidelity === "expanded";
}
