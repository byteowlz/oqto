/**
 * Derives the kernel's ViewportClass from the host viewport. Below the
 * midsize threshold the responsive projection merges inline neighbors
 * (ADR-0042 ladder); above it, overflowing Arrangements scroll.
 */

import { useSyncExternalStore } from "react";
import {
	readViewportSize,
	subscribeViewportSize,
} from "../../platform/viewport";
import type { ViewportClass } from "../index";

/** Logical inline size below which the merge ladder applies. */
export const MERGE_BELOW_INLINE_SIZE = 1100;

let cached: ViewportClass | null = null;

function currentViewportClass(): ViewportClass {
	const size = readViewportSize();
	if (
		cached &&
		cached.inlineSize === size.inlineSize &&
		cached.blockSize === size.blockSize
	) {
		return cached;
	}
	cached = {
		inlineSize: size.inlineSize,
		blockSize: size.blockSize,
		responsive: size.inlineSize < MERGE_BELOW_INLINE_SIZE ? "merge" : "scroll",
	};
	return cached;
}

export function useViewportClass(): ViewportClass {
	return useSyncExternalStore(
		subscribeViewportSize,
		currentViewportClass,
		currentViewportClass,
	);
}
