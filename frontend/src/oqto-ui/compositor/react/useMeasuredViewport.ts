/**
 * The grid solves against the box it actually renders into, not the
 * window: this hook measures the compositor body element and overlays its
 * size on the host-provided ViewportClass (policy, gaps, safe areas), falling
 * back to the host values until the element has a size.
 */

import { useCallback, useRef, useSyncExternalStore } from "react";
import {
	type ElementSize,
	readElementSize,
	subscribeElementSize,
} from "../../platform/element-size";
import type { ViewportClass } from "../index";

export function useMeasuredViewport(
	element: HTMLElement | null,
	fallback: ViewportClass,
): ViewportClass {
	const cache = useRef<ElementSize | null>(null);
	const subscribe = useCallback(
		(listener: () => void) =>
			element ? subscribeElementSize(element, listener) : () => {},
		[element],
	);
	const read = useCallback((): ElementSize | null => {
		if (!element) return null;
		const size = readElementSize(element);
		const last = cache.current;
		if (
			last &&
			last.inlineSize === size.inlineSize &&
			last.blockSize === size.blockSize
		)
			return last;
		cache.current = size;
		return size;
	}, [element]);
	const measured = useSyncExternalStore(subscribe, read, read);
	if (!measured || measured.inlineSize <= 0 || measured.blockSize <= 0)
		return fallback;
	if (
		measured.inlineSize === fallback.inlineSize &&
		measured.blockSize === fallback.blockSize
	)
		return fallback;
	return {
		...fallback,
		inlineSize: measured.inlineSize,
		blockSize: measured.blockSize,
	};
}
