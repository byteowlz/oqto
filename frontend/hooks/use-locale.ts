import { LOCALE_STORAGE_KEY, type Locale } from "@/lib/i18n";
import { useCallback, useEffect, useTransition } from "react";
import { useTranslation } from "react-i18next";

export function useLocale() {
	const { i18n } = useTranslation();
	const currentLocale = (i18n.language === "de" ? "de" : "en") as Locale;
	const [isPending, startTransition] = useTransition();

	useEffect(() => {
		document.documentElement.lang = currentLocale;
	}, [currentLocale]);

	const setLocale = useCallback(
		(newLocale: Locale) => {
			startTransition(() => {
				try {
					window.localStorage.setItem(LOCALE_STORAGE_KEY, newLocale);
				} catch {
					// Ignore storage failures (private mode, denied access).
				}
				void i18n.changeLanguage(newLocale);
				document.documentElement.lang = newLocale;
			});
		},
		[i18n],
	);

	return {
		locale: currentLocale,
		setLocale,
		isPending,
	};
}
