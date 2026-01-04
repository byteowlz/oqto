import i18next from "i18next";
import { initReactI18next } from "react-i18next";

import deMessages from "@/messages/de.json";
import enMessages from "@/messages/en.json";

export type Locale = "en" | "de";

export const locales: Locale[] = ["en", "de"];
export const defaultLocale: Locale = "de";
export const LOCALE_STORAGE_KEY = "locale";

export const i18n = i18next;

export function initI18n() {
	if (i18n.isInitialized) return i18n;

	i18n.use(initReactI18next).init({
		resources: {
			en: { translation: enMessages },
			de: { translation: deMessages },
		},
		lng: defaultLocale,
		fallbackLng: "en",
		interpolation: { escapeValue: false },
	});

	return i18n;
}

export function resolveStoredLocale(): Locale {
	if (typeof window === "undefined") return defaultLocale;
	try {
		const stored = window.localStorage.getItem(LOCALE_STORAGE_KEY);
		if (stored === "en" || stored === "de") return stored;
	} catch {
		// Ignore storage errors (private mode, denied access).
	}
	return defaultLocale;
}
