/**
 * The classic OqtoUI preset composed through the ADR-0041 compositor with
 * OG-shell parity chrome: desktop renders the Container grid; the mobile
 * Screen Mode projects the same Content into one destination with a
 * full-screen navigation drawer and the mobile top bar. Layout persists
 * device-locally; every change is a semantic transaction.
 */

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type {
	OqtoUiConfigResolution,
	OqtoUiPlatform,
	OqtoUiSnapshot,
	SessionOverview,
	UiNavigation,
	WorkDirectory,
} from "../platform/contracts";
import { DEFAULT_OQTO_UI_CONFIG } from "../platform/contracts";
import {
	type LayoutDocumentStore,
	layoutStorageKey,
} from "../platform/layout-storage";
import { useThemeRoot } from "../theme/useThemeRoot";
import { JETBRAINS_MONO_STACK, type OqtoUiUserTheme } from "../theme/userTheme";
import { LoadedCompositorShell } from "./LoadedCompositorShell";
import { Splash } from "./Splash";
import {
	SESSIONS_CONTENT,
	SETTINGS_CONTENT,
	chatContent,
	filesContent,
} from "./compositorRefs";
import "./shell.css";
import "./compositor-shell.css";

type CompositorShellProps = {
	platform: OqtoUiPlatform;
	workDirectoryId: string | null;
	sessionId: string | null;
	mobileView: string;
	schemeId: string | null;
	workAreaTab: string;
	storage: LayoutDocumentStore;
	onNavigate: (next: UiNavigation) => void;
};

export function CompositorShell({
	platform,
	workDirectoryId,
	sessionId,
	mobileView,
	schemeId: requestedSchemeId,
	workAreaTab,
	storage,
	onNavigate,
}: CompositorShellProps) {
	const { t } = useTranslation();
	const [userTheme, setUserTheme] = useState<OqtoUiUserTheme>({});
	const configQuery = useQuery({
		queryKey: ["oqto-ui-config", platform.id],
		queryFn: () => platform.loadUiConfig(),
		staleTime: 30_000,
		retry: false,
	});
	const resolvedConfig = configQuery.data ?? DEFAULT_OQTO_UI_CONFIG;
	const config = resolvedConfig.config;
	const configuredRadius = { square: "0px", compact: "4px", soft: "10px" }[
		config.appearance.radius
	];
	const configuredTheme: OqtoUiUserTheme = {
		...userTheme,
		radius: userTheme.radius ?? configuredRadius,
		fontSans: userTheme.fontSans ?? JETBRAINS_MONO_STACK,
		fontMono: userTheme.fontMono ?? JETBRAINS_MONO_STACK,
	};
	const { schemeId, rootRef, rootEl } = useThemeRoot(
		requestedSchemeId ?? config.appearance.scheme,
		configuredTheme,
	);
	const snapshotQuery = useQuery({
		queryKey: ["oqto-ui-compositor", platform.id, sessionId],
		queryFn: () => platform.load(sessionId),
		placeholderData: keepPreviousData,
		staleTime: 30_000,
	});
	const snapshot = snapshotQuery.data;
	if (!snapshot) {
		return (
			<div className="wb-shell" data-state="loading" ref={rootRef}>
				<Splash
					schemeId={schemeId}
					error={snapshotQuery.isError}
					onRetry={() => void snapshotQuery.refetch()}
				/>
			</div>
		);
	}
	const directories = snapshot.workDirectories;
	const directory =
		directories.find((item) => item.id === workDirectoryId) ??
		directories.find((item) =>
			item.sessions.some((session) => session.id === snapshot.activeSessionId),
		) ??
		directories[0];
	const session =
		directory?.sessions.find(
			(item) => item.id === (sessionId ?? snapshot.activeSessionId),
		) ?? directory?.sessions[0];
	if (!directory || !session) {
		return (
			<div className="wb-shell" data-state="empty" ref={rootRef}>
				<div className="wb-shell-notice">
					<p>{t("oqtoUi.noSessions")}</p>
				</div>
			</div>
		);
	}
	return (
		<LoadedCompositorShell
			snapshot={snapshot}
			platform={platform}
			context={{ directory, session }}
			navigation={{ mobileView, workAreaTab }}
			storage={storage}
			theme={{
				schemeId,
				rootRef,
				rootEl,
				userTheme,
				onUserTheme: setUserTheme,
			}}
			resolvedConfig={resolvedConfig}
			onNavigate={onNavigate}
		/>
	);
}
