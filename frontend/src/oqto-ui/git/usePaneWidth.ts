/**
 * The pane's own width, observed rather than guessed, so a Container that
 * is resized changes what its Content shows. Measurement is renderer input
 * only: it never reaches the layout document.
 */

import { useMountEffect } from "@/hooks/use-mount-effect";
import { useCallback, useRef, useState } from "react";
import {
	readElementSize,
	subscribeElementSize,
} from "../platform/element-size";

export interface PaneWidth {
	readonly ref: (element: HTMLElement | null) => void;
	readonly width: number;
}

export function usePaneWidth(): PaneWidth {
	const [width, setWidth] = useState(0);
	const element = useRef<HTMLElement | null>(null);
	const unobserve = useRef<(() => void) | null>(null);

	const ref = useCallback((next: HTMLElement | null) => {
		unobserve.current?.();
		element.current = next;
		if (!next) {
			unobserve.current = null;
			return;
		}
		const measure = () => setWidth(readElementSize(next).inlineSize);
		measure();
		unobserve.current = subscribeElementSize(next, measure);
	}, []);

	useMountEffect(() => () => unobserve.current?.());

	return { ref, width };
}
