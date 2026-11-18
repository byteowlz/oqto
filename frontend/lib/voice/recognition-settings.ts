import type {
	RecognitionLanguage,
	RecognitionProvider,
} from "@byteowlz/ears-browser";
import { DEFAULT_WAKE_SOUND, type WakeSound, isWakeSound } from "./wake-sounds";

export interface RecognitionSettings {
	provider: RecognitionProvider;
	language: RecognitionLanguage;
	/**
	 * Wake phrase listening (waqy). Off unless the user opts in; a detection only
	 * starts dictation, exactly like pressing the microphone button.
	 */
	wakeEnabled: boolean;
	/** Phrase id from the release manifest, e.g. "hey_oqto". */
	wakePhrase: string;
	/** Show the live wake score, trigger score and threshold slider. */
	wakeDebug: boolean;
	/** Local threshold override in (0, 1); null uses the release threshold. */
	wakeThreshold: number | null;
	/** Send the dictation by itself after a pause in speech. */
	autoSendEnabled: boolean;
	/** Pause after the last speech before the send; one of AUTO_SEND_PAUSES_MS. */
	autoSendPauseMs: number;
	/** Input device id, or null for the system default input. */
	microphoneId: string | null;
	/** Hands-free dictation ends after this much silence; 0 keeps it listening. */
	wakeSilenceTimeoutMs: number;
	/** Confirmation sound when the wake phrase is heard; "off" is silent. */
	wakeSound: WakeSound;
	/** Sound right before an auto-send; off unless chosen. */
	sendSound: WakeSound;
}
export const RECOGNITION_SETTINGS_KEY = "oqto-recognition-settings";
export const DEFAULT_RECOGNITION_SETTINGS: RecognitionSettings = {
	provider: "moonshine",
	language: "en",
	wakeEnabled: false,
	wakePhrase: "hey_oqto",
	wakeDebug: false,
	wakeThreshold: null,
	autoSendEnabled: false,
	autoSendPauseMs: 2000,
	microphoneId: null,
	wakeSilenceTimeoutMs: 20_000,
	wakeSound: DEFAULT_WAKE_SOUND,
	sendSound: "off",
};

/** Offered silence timeouts for a wake-triggered dictation; 0 means never. */
export const WAKE_SILENCE_TIMEOUTS_MS = [
	0, 10_000, 20_000, 30_000, 60_000,
] as const;

/** Offered pause lengths; a stored value outside this list falls back. */
export const AUTO_SEND_PAUSES_MS = [1000, 2000, 3000, 5000] as const;

export function parseRecognitionSettings(raw: string): RecognitionSettings {
	const stored = JSON.parse(raw);
	return {
		provider: stored?.provider === "ears" ? "ears" : "moonshine",
		language: stored?.language === "de" ? "de" : "en",
		wakeEnabled: stored?.wakeEnabled === true,
		wakePhrase:
			typeof stored?.wakePhrase === "string" && stored.wakePhrase
				? stored.wakePhrase
				: "hey_oqto",
		autoSendEnabled: stored?.autoSendEnabled === true,
		autoSendPauseMs: (AUTO_SEND_PAUSES_MS as readonly number[]).includes(
			stored?.autoSendPauseMs,
		)
			? stored.autoSendPauseMs
			: 2000,
		wakeSilenceTimeoutMs: (
			WAKE_SILENCE_TIMEOUTS_MS as readonly number[]
		).includes(stored?.wakeSilenceTimeoutMs)
			? stored.wakeSilenceTimeoutMs
			: 20_000,
		wakeSound: isWakeSound(stored?.wakeSound)
			? stored.wakeSound
			: DEFAULT_WAKE_SOUND,
		sendSound: isWakeSound(stored?.sendSound) ? stored.sendSound : "off",
		microphoneId:
			typeof stored?.microphoneId === "string" && stored.microphoneId
				? stored.microphoneId
				: null,
		wakeDebug: stored?.wakeDebug === true,
		wakeThreshold:
			typeof stored?.wakeThreshold === "number" &&
			stored.wakeThreshold > 0 &&
			stored.wakeThreshold < 1
				? stored.wakeThreshold
				: null,
	};
}
export function reportRecognitionStorageError(error: unknown) {
	console.warn(
		"[Dictation] Recognition settings could not be persisted",
		error,
	);
}
