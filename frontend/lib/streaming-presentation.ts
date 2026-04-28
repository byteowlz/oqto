"use client";

import { useSyncExternalStore } from "react";

export type StreamingPresentationMode = "chunked" | "smooth" | "raw";

const STORAGE_KEY = "oqto:streamingPresentationMode";

// Raw-only presentation is now the sole supported mode.
let currentMode: StreamingPresentationMode = "raw";
const listeners = new Set<() => void>();

function readStoredMode(): StreamingPresentationMode {
	return "raw";
}

function ensureHydrated() {
	if (typeof window === "undefined") return;
	currentMode = readStoredMode();
}

function emit() {
	for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

function getSnapshot() {
	return currentMode;
}

function getServerSnapshot(): StreamingPresentationMode {
	return "raw";
}

export function setStreamingPresentationMode(_mode: StreamingPresentationMode) {
	currentMode = "raw";
	if (typeof window !== "undefined") {
		try {
			window.localStorage.setItem(STORAGE_KEY, "raw");
		} catch {
			// ignore storage failures
		}
	}
	emit();
}

export function useStreamingPresentation() {
	ensureHydrated();
	const mode = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
	return {
		mode,
		setMode: setStreamingPresentationMode,
		isSmooth: false,
		isRaw: true,
	};
}
