import { type DependencyList, useEffect } from "react";

/**
 * Runs reset logic whenever a dialog/surface is opened.
 */
export function useResetOnOpen(
	open: boolean,
	reset: () => void,
	dependencies: DependencyList = [],
): void {
	// useeffect-guardrail: allow - open-state driven reset hook for dialogs
	useEffect(() => {
		if (!open) return;
		reset();
	}, [open, reset, ...dependencies]);
}
