import i18next from "i18next";
import { initReactI18next } from "react-i18next";

import deMessages from "@/messages/de.json";
import enMessages from "@/messages/en.json";

export type Locale = "en" | "de";

export const locales: Locale[] = ["en", "de"];
export const defaultLocale: Locale = "en";
export const LOCALE_STORAGE_KEY = "locale";

export const i18n = i18next;

export function initI18n() {
	if (i18n.isInitialized) return i18n;

	// Resolve here, not only in UIProvider: the auth routes render outside the
	// provider and would otherwise always fall back to the default locale.
	const initialLocale = resolveStoredLocale();

	i18n.use(initReactI18next).init({
		resources: {
			en: { translation: enMessages },
			de: { translation: deMessages },
		},
		lng: initialLocale,
		fallbackLng: "en",
		interpolation: { escapeValue: false },
	});

	if (typeof document !== "undefined") {
		document.documentElement.lang = initialLocale;
	}

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
