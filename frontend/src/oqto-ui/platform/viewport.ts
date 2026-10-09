/**
 * Host viewport measurement for the web adapter. Observed sizes are
 * renderer input only; they never flow into layout state.
 */

export interface ViewportSize {
	readonly inlineSize: number;
	readonly blockSize: number;
}

const FALLBACK: ViewportSize = { inlineSize: 1280, blockSize: 800 };

export function readViewportSize(): ViewportSize {
	if (typeof window === "undefined") return FALLBACK;
	return { inlineSize: window.innerWidth, blockSize: window.innerHeight };
}

export function subscribeViewportSize(listener: () => void): () => void {
	if (typeof window === "undefined") return () => {};
	window.addEventListener("resize", listener);
	return () => window.removeEventListener("resize", listener);
}
