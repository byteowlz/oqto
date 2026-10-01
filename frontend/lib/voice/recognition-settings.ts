import type {
	RecognitionLanguage,
	RecognitionProvider,
} from "@byteowlz/ears-browser";

export interface RecognitionSettings {
	provider: RecognitionProvider;
	language: RecognitionLanguage;
}
export const RECOGNITION_SETTINGS_KEY = "oqto-recognition-settings";
export const DEFAULT_RECOGNITION_SETTINGS: RecognitionSettings = {
	provider: "moonshine",
	language: "en",
};

export function parseRecognitionSettings(raw: string): RecognitionSettings {
	const stored = JSON.parse(raw);
	return {
		provider: stored?.provider === "ears" ? "ears" : "moonshine",
		language: stored?.language === "de" ? "de" : "en",
	};
}
export function reportRecognitionStorageError(error: unknown) {
	console.warn(
		"[Dictation] Recognition settings could not be persisted",
		error,
	);
}
