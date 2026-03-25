import { useEffect, useRef } from "react";

/**
 * Subscribe to a document-level event. The handler ref is kept stable
 * so the listener is only added/removed when `enabled` changes.
 *
 * This is the approved wrapper for document.addEventListener effects.
 */
export function useDocumentEvent<K extends keyof DocumentEventMap>(
	type: K,
	handler: (event: DocumentEventMap[K]) => void,
	enabled = true,
): void {
	const handlerRef = useRef(handler);
	handlerRef.current = handler;

	// useeffect-guardrail: allow — external DOM subscription, no alternative
	useEffect(() => {
		if (!enabled) return;
		const listener = (e: DocumentEventMap[K]) => handlerRef.current(e);
		document.addEventListener(type, listener);
		return () => document.removeEventListener(type, listener);
	}, [type, enabled]);
}
