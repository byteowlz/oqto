/**
 * Host element measurement for the web adapter. Observed sizes are
 * renderer input only; they never flow into layout state.
 */

export interface ElementSize {
	readonly inlineSize: number;
	readonly blockSize: number;
}

export function readElementSize(element: Element): ElementSize {
	const rect = element.getBoundingClientRect();
	return { inlineSize: rect.width, blockSize: rect.height };
}

export function subscribeElementSize(
	element: Element,
	listener: () => void,
): () => void {
	if (typeof ResizeObserver === "undefined") return () => {};
	const observer = new ResizeObserver(listener);
	observer.observe(element);
	return () => observer.disconnect();
}
