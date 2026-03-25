import { type RefObject, useEffect, useRef, useState } from "react";

/**
 * Fires a callback once when the target element enters the viewport.
 * Returns a ref to attach to the target element.
 *
 * This is the approved wrapper for IntersectionObserver effects.
 */
export function useIntersectionOnce(
	callback: () => void,
	options?: { rootMargin?: string; enabled?: boolean },
): RefObject<HTMLDivElement | null> {
	const ref = useRef<HTMLDivElement | null>(null);
	const firedRef = useRef(false);
	const callbackRef = useRef(callback);
	callbackRef.current = callback;

	const enabled = options?.enabled ?? true;
	const rootMargin = options?.rootMargin ?? "100px";

	// useeffect-guardrail: allow — external DOM API (IntersectionObserver), no alternative
	useEffect(() => {
		if (!enabled || firedRef.current || !ref.current) return;

		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0]?.isIntersecting && !firedRef.current) {
					firedRef.current = true;
					callbackRef.current();
					observer.disconnect();
				}
			},
			{ rootMargin },
		);

		observer.observe(ref.current);
		return () => observer.disconnect();
	}, [enabled, rootMargin]);

	return ref;
}
