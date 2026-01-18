"use client";

import {
	type AppDefinition,
	type Locale,
	type LocalizedText,
	appRegistry,
} from "@/lib/app-registry";
import { i18n, resolveStoredLocale } from "@/lib/i18n";
import {
	type ReactNode,
	createContext,
	useCallback,
	useContext,
	useEffect,
	useMemo,
	useState,
} from "react";

interface UIContextValue {
	apps: AppDefinition[];
	activeAppId: string;
	setActiveAppId: (id: string) => void;
	activeApp?: AppDefinition;
	locale: Locale;
	setLocale: (locale: Locale) => void;
	resolveText: (value: LocalizedText) => string;
}

const UIContext = createContext<UIContextValue | null>(null);

const LAST_APP_KEY = "octo:lastActiveApp";

export function UIProvider({ children }: { children: ReactNode }) {
	const [locale, setLocaleState] = useState<Locale>("de");
	const apps = useMemo(() => appRegistry.getAllApps(), []);
	// Restore last active app from localStorage, default to "sessions" (chat view)
	const [activeAppId, setActiveAppIdRaw] = useState(() => {
		if (typeof window !== "undefined") {
			try {
				const stored = localStorage.getItem(LAST_APP_KEY);
				if (stored && apps.some((app) => app.id === stored)) {
					return stored;
				}
			} catch {
				// Ignore localStorage errors
			}
		}
		// Default to sessions app to show the most recent chat
		return apps.find((app) => app.id === "sessions")?.id ?? apps[0]?.id ?? "";
	});
	const activeApp = apps.find((app) => app.id === activeAppId) ?? apps[0];

	// Persist active app to localStorage
	const setActiveAppId = useCallback((id: string) => {
		setActiveAppIdRaw(id);
		if (typeof window !== "undefined") {
			try {
				localStorage.setItem(LAST_APP_KEY, id);
			} catch {
				// Ignore localStorage errors
			}
		}
	}, []);

	useEffect(() => {
		const initialLocale = resolveStoredLocale();
		setLocaleState(initialLocale);
		document.documentElement.lang = initialLocale;
		void i18n.changeLanguage(initialLocale);
	}, []);

	const setLocale = useCallback((next: Locale) => {
		setLocaleState(next);
		document.documentElement.lang = next;
		void i18n.changeLanguage(next);
		try {
			window.localStorage.setItem("locale", next);
		} catch {
			// ignore storage failures
		}
	}, []);

	const resolveText = useCallback(
		(value: LocalizedText) => {
			if (typeof value === "string") return value;
			return locale === "en" ? value.en : value.de;
		},
		[locale],
	);

	const value = useMemo(
		() => ({
			apps,
			activeAppId,
			setActiveAppId,
			activeApp,
			locale,
			setLocale,
			resolveText,
		}),
		[apps, activeAppId, setActiveAppId, activeApp, locale, setLocale, resolveText],
	);

	return <UIContext.Provider value={value}>{children}</UIContext.Provider>;
}

export function useUIContext() {
	const context = useContext(UIContext);
	if (!context) {
		throw new Error("useUIContext must be used within a UIProvider");
	}
	return context;
}

// Selective hooks for performance - only re-render when specific values change
export function useLocale() {
	const { locale, setLocale, resolveText } = useUIContext();
	return { locale, setLocale, resolveText };
}

export function useActiveApp() {
	const { apps, activeAppId, setActiveAppId, activeApp } = useUIContext();
	return { apps, activeAppId, setActiveAppId, activeApp };
}
