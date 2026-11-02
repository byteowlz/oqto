import { useCallback, useRef, useState, useSyncExternalStore } from "react";
import {
	type ElementSize,
	readElementSize,
	subscribeElementSize,
} from "../../platform/element-size";

const FALLBACK: ElementSize = { inlineSize: 1200, blockSize: 760 };
export interface CanvasSize {
	readonly size: ElementSize;
	ref(element: HTMLElement | null): void;
}
/** Canvas box via the platform adapter; drives fit-to-view scale only. */
export function useCanvasSize(): CanvasSize {
	const [element, setElement] = useState<HTMLElement | null>(null);
	const cached = useRef(FALLBACK);
	const subscribe = useCallback(
		(listener: () => void) =>
			element ? subscribeElementSize(element, listener) : () => {},
		[element],
	);
	const read = useCallback(() => {
		if (!element) return cached.current;
		const next = readElementSize(element);
		if (!next.inlineSize || !next.blockSize) return cached.current;
		if (
			next.inlineSize !== cached.current.inlineSize ||
			next.blockSize !== cached.current.blockSize
		)
			cached.current = next;
		return cached.current;
	}, [element]);
	const size = useSyncExternalStore(subscribe, read, read);
	return { size, ref: setElement };
}
