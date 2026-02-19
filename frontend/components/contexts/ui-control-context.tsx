"use client";

import {
	SpotlightOverlay,
	type SpotlightState,
	type SpotlightTour,
} from "@/components/spotlight/SpotlightOverlay";
import { useApp } from "@/hooks/use-app";
import type { WsEvent } from "@/lib/ws-client";
import { useTheme } from "next-themes";
import {
	type ReactNode,
	createContext,
	useCallback,
	useContext,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useNavigate } from "react-router-dom";

export type SessionView =
	| "chat"
	| "files"
	| "terminal"
	| "tasks"
	| "memories"
	| "settings"
	| "canvas"
	| "voice";

export type ExpandedView =
	| "preview"
	| "canvas"
	| "terminal"
	| "memories"
	| null;

export interface SessionUiControls {
	setActiveView: (view: SessionView) => void;
	setExpandedView: (view: ExpandedView) => void;
	setRightSidebarCollapsed: (collapsed: boolean) => void;
}

interface UIControlContextValue {
	navigate: (path: string, replace?: boolean) => void;
	switchSession: (sessionId: string, mode?: "main" | "pi") => void;
	switchView: (view: SessionView) => void;
	openPalette: (open?: boolean) => void;
	toggleSidebar: (collapsed?: boolean) => void;
	setPanel: (view?: ExpandedView, collapsed?: boolean) => void;
	registerSessionControls: (controls: SessionUiControls | null) => void;
	spotlight: SpotlightState | null;
	tour: SpotlightTour | null;
}

const UIControlContext = createContext<UIControlContextValue | null>(null);

interface UIControlProviderProps {
	children: ReactNode;
	sidebarCollapsed: boolean;
	setSidebarCollapsed: (collapsed: boolean) => void;
	setCommandPaletteOpen: (open: boolean) => void;
}

export function UIControlProvider({
	children,
	sidebarCollapsed,
	setSidebarCollapsed,
	setCommandPaletteOpen,
}: UIControlProviderProps) {
	const { setTheme, resolvedTheme } = useTheme();
	const navigateRouter = useNavigate();
	const {
		locale,
		setActiveAppId,
		setSelectedChatSessionId,
		createNewChat,
		setLocale,
	} = useApp();
	const sessionControlsRef = useRef<SessionUiControls | null>(null);
	const pendingViewRef = useRef<SessionView | null>(null);
	const pendingPanelRef = useRef<{
		view?: ExpandedView;
		collapsed?: boolean;
	} | null>(null);
	const [spotlight, setSpotlight] = useState<SpotlightState | null>(null);
	const [tour, setTour] = useState<SpotlightTour | null>(null);

	const registerSessionControls = useCallback(
		(controls: SessionUiControls | null) => {
			sessionControlsRef.current = controls;
			if (controls && pendingViewRef.current) {
				controls.setActiveView(pendingViewRef.current);
				pendingViewRef.current = null;
			}
			if (controls && pendingPanelRef.current) {
				const { view, collapsed } = pendingPanelRef.current;
				if (view !== undefined) {
					controls.setExpandedView(view ?? null);
				}
				if (collapsed !== undefined) {
					controls.setRightSidebarCollapsed(collapsed);
				}
				pendingPanelRef.current = null;
			}
		},
		[],
	);

	const navigate = useCallback(
		(path: string, replace = false) => {
			navigateRouter(path, { replace });
		},
		[navigateRouter],
	);

	const switchSession = useCallback(
		(sessionId: string, _mode?: "main" | "pi") => {
			setActiveAppId("sessions");
			setSelectedChatSessionId(sessionId);
		},
		[setActiveAppId, setSelectedChatSessionId],
	);

	const switchView = useCallback(
		(view: SessionView) => {
			setActiveAppId("sessions");
			if (sessionControlsRef.current) {
				sessionControlsRef.current.setActiveView(view);
			} else {
				pendingViewRef.current = view;
			}
		},
		[setActiveAppId],
	);

	const openPalette = useCallback(
		(open = true) => {
			setCommandPaletteOpen(open);
		},
		[setCommandPaletteOpen],
	);

	const toggleSidebar = useCallback(
		(collapsed?: boolean) => {
			if (collapsed === undefined) {
				setSidebarCollapsed(!sidebarCollapsed);
				return;
			}
			setSidebarCollapsed(collapsed);
		},
		[setSidebarCollapsed, sidebarCollapsed],
	);

	const setPanel = useCallback((view?: ExpandedView, collapsed?: boolean) => {
		if (sessionControlsRef.current) {
			if (view !== undefined) {
				sessionControlsRef.current.setExpandedView(view ?? null);
			}
			if (collapsed !== undefined) {
				sessionControlsRef.current.setRightSidebarCollapsed(collapsed);
			}
			return;
		}
		pendingPanelRef.current = { view, collapsed };
	}, []);

	const executePaletteCommand = useCallback(
		async (command: string, args?: Record<string, unknown>) => {
			switch (command) {
				case "new_chat":
					await createNewChat();
					setActiveAppId("sessions");
					return;
				case "toggle_theme": {
					const next = resolvedTheme === "dark" ? "light" : "dark";
					setTheme(next);
					return;
				}
				case "set_theme": {
					const theme = typeof args?.theme === "string" ? args?.theme : null;
					if (theme) setTheme(theme);
					return;
				}
				case "toggle_locale":
					setLocale(locale === "de" ? "en" : "de");
					return;
				case "set_locale": {
					const locale = typeof args?.locale === "string" ? args?.locale : null;
					if (locale === "de" || locale === "en") {
						setLocale(locale);
					}
					return;
				}
				case "open_app": {
					const appId = typeof args?.appId === "string" ? args.appId : null;
					if (appId) setActiveAppId(appId);
					return;
				}
				case "select_session": {
					const sessionId =
						typeof args?.sessionId === "string" ? args.sessionId : null;
					if (sessionId) {
						setActiveAppId("sessions");
						setSelectedChatSessionId(sessionId);
					}
					return;
				}
				default:
					console.warn("[ui-control] Unknown palette command:", command);
			}
		},
		[
			createNewChat,
			locale,
			setActiveAppId,
			setLocale,
			setSelectedChatSessionId,
			setTheme,
			resolvedTheme,
		],
	);

	useEffect(() => {
		let unsubscribe: (() => void) | undefined;
		import("@/lib/ws-client").then(({ getWsClient }) => {
			const client = getWsClient();
			unsubscribe = client.onEvent((event: WsEvent) => {
				switch (event.type) {
					case "ui.navigate":
						navigate(event.path, event.replace);
						break;
					case "ui.session":
						switchSession(event.session_id, event.mode as "main" | "pi");
						break;
					case "ui.view":
						switchView(event.view as SessionView);
						break;
					case "ui.palette":
						openPalette(event.open);
						break;
					case "ui.palette_exec":
						void executePaletteCommand(
							event.command,
							(event.args as Record<string, unknown>) ?? undefined,
						);
						break;
					case "ui.spotlight":
						if (!event.active || !event.target) {
							setSpotlight(null);
							return;
						}
						setSpotlight({
							active: event.active,
							target: event.target,
							title: event.title ?? undefined,
							description: event.description ?? undefined,
							action: event.action ?? undefined,
							position:
								(event.position as SpotlightState["position"]) ?? "auto",
						});
						break;
					case "ui.tour":
						if (!event.active || event.steps.length === 0) {
							setTour(null);
							return;
						}
						setTour({
							active: event.active,
							steps: event.steps.map((step) => ({
								active: true,
								target: step.target,
								title: step.title ?? undefined,
								description: step.description ?? undefined,
								action: step.action ?? undefined,
								position:
									(step.position as SpotlightState["position"]) ?? "auto",
							})),
							index: event.start_index ?? 0,
						});
						break;
					case "ui.sidebar":
						toggleSidebar(event.collapsed ?? undefined);
						break;
					case "ui.panel":
						setPanel(
							event.view === null
								? null
								: (event.view as ExpandedView | undefined),
							event.collapsed ?? undefined,
						);
						break;
					case "ui.theme":
						setTheme(event.theme);
						break;
					default:
						break;
				}
			});
		});

		return () => {
			unsubscribe?.();
		};
	}, [
		executePaletteCommand,
		navigate,
		openPalette,
		setPanel,
		setTheme,
		switchSession,
		switchView,
		toggleSidebar,
	]);

	const value = useMemo(
		() => ({
			navigate,
			switchSession,
			switchView,
			openPalette,
			toggleSidebar,
			setPanel,
			registerSessionControls,
			spotlight,
			tour,
		}),
		[
			navigate,
			switchSession,
			switchView,
			openPalette,
			toggleSidebar,
			setPanel,
			registerSessionControls,
			spotlight,
			tour,
		],
	);

	const handleSpotlightClose = useCallback(() => {
		setSpotlight(null);
	}, []);

	const handleTourClose = useCallback(() => {
		setTour(null);
	}, []);

	const handleTourNext = useCallback(() => {
		setTour((prev) => {
			if (!prev) return null;
			const nextIndex = Math.min(prev.index + 1, prev.steps.length - 1);
			return { ...prev, index: nextIndex };
		});
	}, []);

	const handleTourPrev = useCallback(() => {
		setTour((prev) => {
			if (!prev) return null;
			const nextIndex = Math.max(prev.index - 1, 0);
			return { ...prev, index: nextIndex };
		});
	}, []);

	const activeSpotlight = tour
		? {
				active: true,
				...tour.steps[tour.index],
			}
		: spotlight;

	return (
		<UIControlContext.Provider value={value}>
			{children}
			<SpotlightOverlay
				spotlight={activeSpotlight}
				tour={tour}
				onClose={tour ? handleTourClose : handleSpotlightClose}
				onNext={tour ? handleTourNext : undefined}
				onPrev={tour ? handleTourPrev : undefined}
			/>
		</UIControlContext.Provider>
	);
}

export function useUIControl() {
	const context = useContext(UIControlContext);
	if (!context) {
		throw new Error("useUIControl must be used within a UIControlProvider");
	}
	return context;
}
