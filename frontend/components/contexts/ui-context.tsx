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

// Default no-op functions for HMR resilience
const noop = () => {};

// During HMR, components may briefly render without the provider.
// This default value prevents crashes while React Fast Refresh retries.
const defaultUIContext: UIContextValue = {
	apps: [],
	activeAppId: "",
	setActiveAppId: noop,
	activeApp: undefined,
	locale: "en",
	setLocale: noop,
	resolveText: (value) => (typeof value === "string" ? value : value.en),
};

const UIContext = createContext<UIContextValue>(defaultUIContext);

export function UIProvider({ children }: { children: ReactNode }) {
	const [locale, setLocaleState] = useState<Locale>("en");
	const apps = useMemo(() => appRegistry.getAllApps(), []);
	// Always start with sessions app - last active chat is restored by SessionContext
	const [activeAppId, setActiveAppIdRaw] = useState(() => {
		return apps.find((app) => app.id === "sessions")?.id ?? apps[0]?.id ?? "";
	});
	const activeApp = apps.find((app) => app.id === activeAppId) ?? apps[0];

	const setActiveAppId = useCallback((id: string) => {
		setActiveAppIdRaw(id);
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
		[
			apps,
			activeAppId,
			setActiveAppId,
			activeApp,
			locale,
			setLocale,
			resolveText,
		],
	);

	return <UIContext.Provider value={value}>{children}</UIContext.Provider>;
}

export function useUIContext() {
	return useContext(UIContext);
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
