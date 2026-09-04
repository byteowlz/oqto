/**
 * Expanded content moves from the utility pane into the main work area.
 * It must never remain mounted in both surfaces: App frames and other Views
 * own live state and side effects rather than being safe visual mirrors.
 */
export function shouldMountInUtilityPane(
	activeView: string,
	expandedView: string | null,
	view: string,
): boolean {
	return activeView === view && expandedView !== view;
}
