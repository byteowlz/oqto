import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChatWorkspace } from "../chat/ChatWorkspace";
import type { PreviewSelection } from "../chat/ResourcePreviewPane";
import { TaskProgress } from "../chat/TaskProgress";
import { FilesPane } from "../files/FilesPane";
import { GalleryPane } from "../gallery/GalleryPane";
import { CornerModeChrome } from "../layout/CornerModeChrome";
import { MobileTopBar } from "../layout/MobileTopBar";
import type {
	OqtoUiConfigResolution,
	OqtoUiPlatform,
	OqtoUiSnapshot,
	UiNavigation,
} from "../platform/contracts";
import { DEFAULT_OQTO_UI_CONFIG } from "../platform/contracts";
import { NavigationRail } from "../sessions/NavigationRail";
import { SessionStatusBar } from "../sessions/SessionStatusBar";
import { SettingsPane } from "../theme/SettingsPane";
import { useThemeRoot } from "../theme/useThemeRoot";
import { JETBRAINS_MONO_STACK, type OqtoUiUserTheme } from "../theme/userTheme";
import { Splash } from "./Splash";
import { useConfiguredBindings } from "./useConfiguredBindings";
import "./shell.css";

type OqtoUiShellProps = {
	platform: OqtoUiPlatform;
	workDirectoryId: string | null;
	sessionId: string | null;
	mobileView: string;
	schemeId: string | null;
	workAreaTab: string;
	onNavigate: (next: UiNavigation) => void;
};

export function OqtoUiShell({
	platform,
	workDirectoryId,
	sessionId,
	mobileView,
	schemeId: requestedSchemeId,
	workAreaTab,
	onNavigate,
}: OqtoUiShellProps) {
	const [sessionsOpen, setSessionsOpen] = useState(false);
	const [userTheme, setUserTheme] = useState<OqtoUiUserTheme>({});
	const configQuery = useQuery({
		queryKey: ["oqto-ui-config", platform.id],
		queryFn: () => platform.loadUiConfig(),
		staleTime: 30_000,
		retry: false,
	});
	const resolvedConfig = configQuery.data ?? DEFAULT_OQTO_UI_CONFIG;
	const config = resolvedConfig.config;
	const configuredRadius = {
		square: "0px",
		compact: "4px",
		soft: "10px",
	}[config.appearance.radius];
	const configuredTheme: OqtoUiUserTheme = {
		...userTheme,
		radius: userTheme.radius ?? configuredRadius,
		// OqtoUI's baseline identity is mono. A user customization may
		// deliberately override either stack, but live config fallback must
		// never silently regress to a proportional system font.
		fontSans: userTheme.fontSans ?? JETBRAINS_MONO_STACK,
		fontMono: userTheme.fontMono ?? JETBRAINS_MONO_STACK,
	};
	const { schemeId, rootRef, rootEl } = useThemeRoot(
		requestedSchemeId ?? config.appearance.scheme,
		configuredTheme,
	);
	useConfiguredBindings(config.bindings, onNavigate);
	const snapshotQuery = useQuery({
		queryKey: ["oqto-ui", platform.id, sessionId],
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

	return (
		<LoadedShell
			snapshot={snapshot}
			platform={platform}
			navigation={{ workDirectoryId, sessionId, mobileView, workAreaTab }}
			theme={{ schemeId, rootRef, rootEl }}
			userTheme={userTheme}
			onUserTheme={setUserTheme}
			resolvedConfig={resolvedConfig}
			shellState={{ sessionsOpen, setSessionsOpen, onNavigate }}
		/>
	);
}

type NavigationState = {
	workDirectoryId: string | null;
	sessionId: string | null;
	mobileView: string;
	workAreaTab: string;
};

type ShellState = {
	sessionsOpen: boolean;
	setSessionsOpen: (open: boolean) => void;
	onNavigate: (next: UiNavigation) => void;
};

type ThemeState = {
	schemeId: string;
	rootRef: (root: HTMLDivElement | null) => void;
	rootEl: HTMLDivElement | null;
};

type LoadedShellProps = {
	snapshot: OqtoUiSnapshot;
	platform: OqtoUiPlatform;
	navigation: NavigationState;
	theme: ThemeState;
	userTheme: OqtoUiUserTheme;
	onUserTheme: (next: OqtoUiUserTheme) => void;
	resolvedConfig: OqtoUiConfigResolution;
	shellState: ShellState;
};

function LoadedShell({
	snapshot,
	platform,
	navigation,
	theme,
	userTheme,
	onUserTheme,
	resolvedConfig,
	shellState,
}: LoadedShellProps) {
	const { schemeId, rootRef, rootEl } = theme;
	const { t } = useTranslation();
	const [settingsOpen, setSettingsOpen] = useState(false);
	const [preview, setPreview] = useState<
		(PreviewSelection & { workDirectoryId: string }) | null
	>(null);
	const { sessionsOpen, setSessionsOpen, onNavigate } = shellState;
	const directories = snapshot.workDirectories;
	const directory =
		directories.find((item) => item.id === navigation.workDirectoryId) ??
		directories.find((item) =>
			item.sessions.some((session) => session.id === snapshot.activeSessionId),
		) ??
		directories[0];
	const session =
		directory?.sessions.find(
			(item) => item.id === (navigation.sessionId ?? snapshot.activeSessionId),
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
	const status = snapshot.environment.statusBar;
	const config = resolvedConfig.config;

	return (
		<div
			className="wb-shell"
			data-sessions-open={sessionsOpen}
			data-files-placement={config.layout.files}
			data-navigator-placement={config.layout.navigator}
			data-density={config.appearance.density}
			data-mobile-mode={config.mobile.mode}
			data-config-source={resolvedConfig.source}
			ref={rootRef}
		>
			<NavigationRail
				workDirectories={directories}
				workDirectoryId={directory.id}
				sessionId={session.id}
				schemeId={schemeId}
				onNavigate={(next) => {
					setSessionsOpen(false);
					onNavigate(next);
				}}
			/>
			<button
				className="wb-sessions-backdrop"
				type="button"
				tabIndex={sessionsOpen ? 0 : -1}
				aria-label={t("oqtoUi.mobile.closeSessions")}
				onClick={() => setSessionsOpen(false)}
			/>
			<div className="wb-content">
				<MobileTopBar
					directory={directory}
					session={session}
					activeView={navigation.mobileView}
					activeTab={navigation.workAreaTab}
					tabs={snapshot.workArea.tabs}
					taskProgress={
						<TaskProgress tasks={session.tasks ?? []} placement="mobile" />
					}
					onOpenSessions={() => setSessionsOpen(true)}
					onNavigate={onNavigate}
				/>
				<CornerModeChrome
					config={config}
					directories={directories}
					directory={directory}
					session={session}
					onNavigate={onNavigate}
				>
					<div className="wb-workarea" data-view={navigation.mobileView}>
						<ChatWorkspace
							platform={platform}
							context={{
								directory,
								session,
								tasks: session.tasks ?? [],
							}}
							workArea={snapshot.workArea}
							workAreaTab={navigation.workAreaTab}
							galleryPane={<GalleryPane resources={snapshot.gallery} />}
							previewState={{
								selection:
									preview?.workDirectoryId === directory.id ? preview : null,
								open: (selection) =>
									setPreview({ ...selection, workDirectoryId: directory.id }),
								close: () => setPreview(null),
							}}
							onNavigate={onNavigate}
						/>
						{settingsOpen ? (
							<SettingsPane
								schemeId={schemeId}
								themeRoot={rootEl}
								userTheme={userTheme}
								resolution={resolvedConfig}
								onChange={onUserTheme}
								onNavigate={onNavigate}
								onClose={() => setSettingsOpen(false)}
							/>
						) : (
							<FilesPane files={snapshot.files} />
						)}
					</div>
				</CornerModeChrome>
				<SessionStatusBar
					status={status}
					session={session}
					models={snapshot.environment.models}
					settingsOpen={settingsOpen}
					onToggleSettings={() => setSettingsOpen((open) => !open)}
				/>
			</div>
		</div>
	);
}
