import type {
	RecognitionLanguage,
	RecognitionProvider,
} from "@byteowlz/ears-browser";

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
}
export const RECOGNITION_SETTINGS_KEY = "oqto-recognition-settings";
export const DEFAULT_RECOGNITION_SETTINGS: RecognitionSettings = {
	provider: "moonshine",
	language: "en",
	wakeEnabled: false,
	wakePhrase: "hey_oqto",
};

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
	};
}
export function reportRecognitionStorageError(error: unknown) {
	console.warn(
		"[Dictation] Recognition settings could not be persisted",
		error,
	);
}
