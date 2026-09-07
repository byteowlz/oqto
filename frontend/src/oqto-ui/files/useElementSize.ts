/**
 * The pane's own box, observed through the platform adapter. Fidelity is
 * derived from the container the pane was allocated, not from the window.
 */

import { useCallback, useRef, useState, useSyncExternalStore } from "react";
import {
	readElementSize,
	subscribeElementSize,
} from "../platform/element-size";

export interface MeasuredElement {
	readonly inlineSize: number;
	/** Attach to the element whose width decides fidelity. */
	ref(element: HTMLElement | null): void;
}

export function useElementSize(): MeasuredElement {
	const [element, setElement] = useState<HTMLElement | null>(null);
	const cached = useRef(0);
	const subscribe = useCallback(
		(listener: () => void) =>
			element ? subscribeElementSize(element, listener) : () => {},
		[element],
	);
	const read = useCallback(() => {
		if (!element) return cached.current;
		const size = readElementSize(element).inlineSize;
		if (size !== cached.current) cached.current = size;
		return cached.current;
	}, [element]);
	const inlineSize = useSyncExternalStore(subscribe, read, read);
	return { inlineSize, ref: setElement };
}
