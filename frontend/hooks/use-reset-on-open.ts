import { type DependencyList, useEffect, useRef } from "react";

/**
 * Runs reset logic whenever a dialog/surface is opened.
 */
export function useResetOnOpen(
	open: boolean,
	reset: () => void,
	dependencies: DependencyList = [],
): void {
	const resetRef = useRef(reset);
	resetRef.current = reset;

	// useeffect-guardrail: allow - open-state driven reset hook for dialogs
	useEffect(() => {
		if (!open) return;
		resetRef.current();
	}, [open, ...dependencies]);
}
