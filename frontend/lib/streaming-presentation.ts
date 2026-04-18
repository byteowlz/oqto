"use client";

import { useSyncExternalStore } from "react";

export type StreamingPresentationMode = "chunked" | "smooth" | "raw";

const STORAGE_KEY = "oqto:streamingPresentationMode";

let currentMode: StreamingPresentationMode = "chunked";
const listeners = new Set<() => void>();

function readStoredMode(): StreamingPresentationMode {
	if (typeof window === "undefined") return "chunked";
	try {
		const stored = window.localStorage.getItem(STORAGE_KEY);
		if (stored === "smooth") return "smooth";
		if (stored === "raw") return "raw";
		return "chunked";
	} catch {
		return "chunked";
	}
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
	return "chunked";
}

export function setStreamingPresentationMode(mode: StreamingPresentationMode) {
	currentMode = mode;
	if (typeof window !== "undefined") {
		try {
			window.localStorage.setItem(STORAGE_KEY, mode);
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
		isSmooth: mode === "smooth",
		isRaw: mode === "raw",
	};
}
