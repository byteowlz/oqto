"use client";

import { useCallback, useEffect, useState } from "react";

export type ChatVerbosity = 1 | 2 | 3;

const STORAGE_KEY = "oqto:chatVerbosity";
const EVENT_NAME = "oqto:chat-verbosity";

function clampVerbosity(value: number): ChatVerbosity {
	if (value <= 1) return 1;
	if (value >= 3) return 3;
	return 2;
}

export function readChatVerbosity(): ChatVerbosity {
	if (typeof window === "undefined") return 3;
	try {
		const stored = window.localStorage.getItem(STORAGE_KEY);
		if (!stored) return 3;
		const parsed = Number.parseInt(stored, 10);
		if (Number.isNaN(parsed)) return 3;
		return clampVerbosity(parsed);
	} catch {
		return 3;
	}
}

export function useChatVerbosity() {
	const [verbosity, setVerbosityState] = useState<ChatVerbosity>(() =>
		readChatVerbosity(),
	);

	const setVerbosity = useCallback((next: ChatVerbosity) => {
		setVerbosityState(next);
		try {
			window.localStorage.setItem(STORAGE_KEY, String(next));
		} catch {
			// ignore storage failures
		}
		window.dispatchEvent(new CustomEvent(EVENT_NAME, { detail: next }));
	}, []);

	useEffect(() => {
		const handleStorage = (event: StorageEvent) => {
			if (event.key !== STORAGE_KEY) return;
			setVerbosityState(readChatVerbosity());
		};
		const handleCustom = (event: Event) => {
			const custom = event as CustomEvent<ChatVerbosity>;
			if (typeof custom.detail === "number") {
				setVerbosityState(clampVerbosity(custom.detail));
			} else {
				setVerbosityState(readChatVerbosity());
			}
		};
		window.addEventListener("storage", handleStorage);
		window.addEventListener(EVENT_NAME, handleCustom);
		return () => {
			window.removeEventListener("storage", handleStorage);
			window.removeEventListener(EVENT_NAME, handleCustom);
		};
	}, []);

	return { verbosity, setVerbosity };
}
