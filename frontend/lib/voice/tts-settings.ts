import { useSyncExternalStore } from "react";

export type TTSProvider = "kokorox" | "phonon";
export interface TTSSettings {
	/** Legacy fields remain exclusively Kokorox settings. */
	voice: string;
	speed: number;
	provider: TTSProvider | "unsupported";
	phonon: { voice: string; speed: number; language: string };
}
export const TTS_SETTINGS_KEY = "oqto-tts-read-aloud-settings";
const CHANGE_EVENT = "oqto:tts-settings-change";
export const DEFAULT_TTS_SETTINGS: TTSSettings = {
	provider: "kokorox",
	voice: "af_heart",
	speed: 1.3,
	phonon: { voice: "default", speed: 1, language: "en" },
};

export function parseTTSSettings(raw: string): TTSSettings {
	const stored = JSON.parse(raw);
	if (!stored || typeof stored !== "object" || Array.isArray(stored))
		throw new Error("Invalid read-aloud settings");
	return {
		...DEFAULT_TTS_SETTINGS,
		...stored,
		provider:
			stored.provider === undefined || stored.provider === "kokorox"
				? "kokorox"
				: stored.provider === "phonon"
					? "phonon"
					: "unsupported",
		phonon: { ...DEFAULT_TTS_SETTINGS.phonon, ...stored.phonon },
	};
}

let snapshot = DEFAULT_TTS_SETTINGS;
let snapshotRaw: string | null | undefined;
let memoryOnly = false;
export function loadTTSSettings(): TTSSettings {
	if (typeof window === "undefined") return DEFAULT_TTS_SETTINGS;
	if (memoryOnly) return snapshot;
	try {
		const raw = window.localStorage.getItem(TTS_SETTINGS_KEY);
		if (raw !== snapshotRaw) {
			snapshotRaw = raw;
			snapshot = raw === null ? DEFAULT_TTS_SETTINGS : parseTTSSettings(raw);
		}
	} catch (error) {
		console.warn("[TTS] Settings unavailable", error);
		// Corrupt/unreadable storage must not silently select a network destination.
		if (snapshot.provider !== "unsupported")
			snapshot = { ...DEFAULT_TTS_SETTINGS, provider: "unsupported" };
	}
	return snapshot;
}

/** Explicit full-state API; legacy voice/speed editors must use saveTTSSettings. */
export function saveReadAloudSettings(settings: TTSSettings) {
	const raw = JSON.stringify(settings);
	snapshot = parseTTSSettings(raw);
	snapshotRaw = raw;
	if (typeof window === "undefined") return;
	try {
		window.localStorage.setItem(TTS_SETTINGS_KEY, raw);
		memoryOnly = false;
	} catch (error) {
		memoryOnly = true;
		console.warn("[TTS] Settings could not be persisted", error);
		// Keep this tab's explicit choice even when storage is blocked.
	}
	window.dispatchEvent(new Event(CHANGE_EVENT));
}

/** A legacy editor's stale snapshot must never change the inference destination. */
export function saveTTSSettings(
	settings: Pick<TTSSettings, "voice" | "speed">,
) {
	saveReadAloudSettings({
		...loadTTSSettings(),
		voice: settings.voice,
		speed: settings.speed,
	});
}

export function updatePhononTTSSettings(patch: Partial<TTSSettings["phonon"]>) {
	const current = loadTTSSettings();
	saveReadAloudSettings({
		...current,
		phonon: { ...current.phonon, ...patch },
	});
}

export function subscribeTTSSettings(onChange: () => void) {
	const storage = (event: StorageEvent) => {
		if (event.key === TTS_SETTINGS_KEY || event.key === null) {
			memoryOnly = false;
			snapshotRaw = undefined;
			onChange();
		}
	};
	window.addEventListener("storage", storage);
	window.addEventListener(CHANGE_EVENT, onChange);
	return () => {
		window.removeEventListener("storage", storage);
		window.removeEventListener(CHANGE_EVENT, onChange);
	};
}

export function selectTTSProvider(provider: TTSProvider) {
	if (provider !== "kokorox" && provider !== "phonon")
		throw new Error("Unsupported read-aloud provider");
	saveReadAloudSettings({ ...loadTTSSettings(), provider });
}

/** All mounted controls share the same provider and per-provider settings. */
export function useTTSSettings() {
	return useSyncExternalStore(
		subscribeTTSSettings,
		loadTTSSettings,
		() => DEFAULT_TTS_SETTINGS,
	);
}
