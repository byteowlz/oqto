import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Activity, Users } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChatWorkspace } from "../chat/ChatWorkspace";
import { TaskProgress } from "../chat/TaskProgress";
import { FilesPane } from "../files/FilesPane";
import { GalleryPane } from "../gallery/GalleryPane";
import { MobileTopBar } from "../layout/MobileTopBar";
import type {
	OqtoUiConfigResolution,
	OqtoUiPlatform,
	OqtoUiSnapshot,
	UiNavigation,
} from "../platform/contracts";
import { DEFAULT_OQTO_UI_CONFIG } from "../platform/contracts";
import { NavigationRail } from "../sessions/NavigationRail";
import { SessionMeta } from "../sessions/SessionMeta";
import { ThemeCustomizer } from "../theme/ThemeCustomizer";
import { ThemePicker } from "../theme/ThemePicker";
import { useThemeRoot } from "../theme/useThemeRoot";
import type { OqtoUiUserTheme } from "../theme/userTheme";
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
			data-config-source={resolvedConfig.source}
			ref={rootRef}
		>
			<NavigationRail
				workDirectories={directories}
				workDirectoryId={directory.id}
				sessionId={session.id}
				themePicker={
					<ThemePicker schemeId={schemeId} onNavigate={onNavigate} />
				}
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
				<div className="wb-workarea" data-view={navigation.mobileView}>
					<ChatWorkspace
						platform={platform}
						directory={directory}
						session={session}
						tasks={session.tasks ?? []}
						workArea={snapshot.workArea}
						workAreaTab={navigation.workAreaTab}
						galleryPane={<GalleryPane resources={snapshot.gallery} />}
						onNavigate={onNavigate}
					/>
					<FilesPane files={snapshot.files} />
				</div>
				<ThemeCustomizer
					themeRoot={rootEl}
					userTheme={userTheme}
					onChange={onUserTheme}
				/>
				<div
					className="wb-config-lens"
					data-invalid={resolvedConfig.diagnostics.length > 0}
				>
					<span>{t("oqtoUi.customization.file")}</span>
					<strong>{config.preset}</strong>
					<span>
						{t("oqtoUi.customization.bindings", {
							count: config.bindings.length,
						})}
					</span>
					{resolvedConfig.diagnostics[0] ? (
						<span title={resolvedConfig.diagnostics[0].message}>
							{resolvedConfig.diagnostics[0].code}
						</span>
					) : null}
				</div>
				{status ? (
					<footer className="wb-statusbar">
						<div className="wb-statusbar__group">
							<span
								className="wb-statusbar__item"
								title={t("oqtoUi.statusBar.runningSessions")}
							>
								<Activity aria-hidden="true" />
								{status.runningSessions}
							</span>
							<SessionMeta
								key={session.id}
								session={session}
								models={snapshot.environment.models}
							/>
						</div>
						<div className="wb-statusbar__group">
							<span
								className="wb-statusbar__item"
								title={t("oqtoUi.statusBar.onlineUsers")}
							>
								<Users aria-hidden="true" />
								{status.onlineUsers}
							</span>
							<span
								className="wb-statusbar__item"
								title={t("oqtoUi.statusBar.runnerLoad")}
							>
								<Activity aria-hidden="true" />
								{status.runnerLoad}
							</span>
							<span
								className="wb-statusbar__item wb-statusbar__item--dim"
								title={t("oqtoUi.statusBar.version")}
							>
								{status.version}
							</span>
						</div>
					</footer>
				) : (
					<footer className="wb-statusbar">
						<div className="wb-statusbar__group">
							<SessionMeta
								key={session.id}
								session={session}
								models={snapshot.environment.models}
							/>
						</div>
					</footer>
				)}
			</div>
		</div>
	);
}
