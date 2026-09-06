/**
 * Loaded compositor shell: OG chrome around the compositor, with the mobile
 * Screen Mode projection below the OG breakpoint. Layout persists
 * device-locally; every change is a semantic transaction.
 */

import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { PreviewSelection } from "../chat/ResourcePreviewPane";
import { TaskProgress } from "../chat/TaskProgress";
import { createClassicPresetLayout } from "../compositor/index";
import { CompositorHost } from "../compositor/react/CompositorHost";
import { bindingsFromConfig } from "../compositor/react/config-bindings";
import {
	type PersistedCompositorStore,
	createPersistedCompositorStore,
} from "../compositor/react/persisted-store";
import { useViewportClass } from "../compositor/react/useViewportClass";
import { CornerModeChrome } from "../layout/CornerModeChrome";
import { MobileTopBar } from "../layout/MobileTopBar";
import type {
	OqtoUiConfigResolution,
	OqtoUiPlatform,
	OqtoUiSnapshot,
	SessionOverview,
	UiNavigation,
	WorkDirectory,
} from "../platform/contracts";
import {
	type LayoutDocumentStore,
	layoutStorageKey,
} from "../platform/layout-storage";
import { SessionStatusBar } from "../sessions/SessionStatusBar";
import type { OqtoUiUserTheme } from "../theme/userTheme";
import { MobileCompositor } from "./MobileCompositor";
import { createContentLabel, createContentRenderer } from "./compositorContent";
import { createChromeLabels } from "./compositorLabels";
import {
	SESSIONS_CONTENT,
	SETTINGS_CONTENT,
	chatContent,
	filesContent,
} from "./compositorRefs";

/** Below this inline size the mobile Screen Mode projection applies (OG breakpoint). */
export const MOBILE_SCREEN_MODE_BELOW = 1024;

/** Idempotent: reveals the Session's Chat, opening it into primary if absent. */
function ensureSessionChat(
	store: PersistedCompositorStore,
	sessionId: string,
): void {
	store.commit([
		{
			type: "open",
			content: chatContent(sessionId),
			target: { role: "primary" },
		},
	]);
}

type LoadedCompositorShellProps = {
	snapshot: OqtoUiSnapshot;
	platform: OqtoUiPlatform;
	context: { directory: WorkDirectory; session: SessionOverview };
	navigation: { mobileView: string; workAreaTab: string };
	storage: LayoutDocumentStore;
	theme: {
		schemeId: string;
		rootRef: (root: HTMLDivElement | null) => void;
		rootEl: HTMLDivElement | null;
		userTheme: OqtoUiUserTheme;
		onUserTheme: (next: OqtoUiUserTheme) => void;
	};
	resolvedConfig: OqtoUiConfigResolution;
	onNavigate: (next: UiNavigation) => void;
};

export function LoadedCompositorShell({
	snapshot,
	platform,
	context,
	navigation,
	storage,
	theme,
	resolvedConfig,
	onNavigate,
}: LoadedCompositorShellProps) {
	const { t } = useTranslation();
	const viewport = useViewportClass();
	const mobile = viewport.inlineSize < MOBILE_SCREEN_MODE_BELOW;
	const [sessionsOpen, setSessionsOpen] = useState(false);
	const [preview, setPreview] = useState<PreviewSelection | null>(null);
	const [store] = useState(() => {
		const created = createPersistedCompositorStore({
			storage,
			key: layoutStorageKey(platform.id, "desktop"),
			fallback: createClassicPresetLayout({
				navigation: [SESSIONS_CONTENT],
				primary: [chatContent(context.session.id)],
				auxiliary: [filesContent(context.directory.id)],
			}),
		});
		ensureSessionChat(created, context.session.id);
		return created;
	});
	const settingsOpen = store
		.getSnapshot()
		.containers.some((c) =>
			c.stack.some((item) => item.id === SETTINGS_CONTENT.id),
		);
	const navigate = useCallback(
		(next: UiNavigation) => {
			setSessionsOpen(false);
			if (next.sessionId) ensureSessionChat(store, next.sessionId);
			onNavigate(next);
		},
		[store, onNavigate],
	);
	const closeSettings = useCallback(() => {
		store.commit([{ type: "close", contentId: SETTINGS_CONTENT.id }]);
	}, [store]);
	const toggleSettings = () => {
		if (settingsOpen) closeSettings();
		else
			store.commit([
				{
					type: "open",
					content: SETTINGS_CONTENT,
					target: { role: "auxiliary" },
				},
			]);
	};
	const previewState = useMemo(
		() => ({
			selection: preview,
			open: setPreview,
			close: () => setPreview(null),
		}),
		[preview],
	);
	const renderContent = useMemo(
		() =>
			createContentRenderer({
				snapshot,
				platform,
				context,
				workAreaTab: navigation.workAreaTab,
				theme,
				resolvedConfig,
				previewState,
				navigate,
				closeSettings,
			}),
		[
			snapshot,
			platform,
			context,
			navigation.workAreaTab,
			theme,
			resolvedConfig,
			previewState,
			navigate,
			closeSettings,
		],
	);
	const contentLabel = useMemo(
		() => createContentLabel(snapshot, t),
		[snapshot, t],
	);
	const labels = useMemo(() => createChromeLabels(t), [t]);
	const keyBindings = useMemo(
		() => bindingsFromConfig(resolvedConfig.config.bindings),
		[resolvedConfig],
	);
	const config = resolvedConfig.config;
	return (
		<div
			className="wb-shell"
			data-compositor={mobile ? "mobile" : "classic"}
			data-sessions-open={sessionsOpen}
			data-density={config.appearance.density}
			data-mobile-mode={config.mobile.mode}
			data-config-source={resolvedConfig.source}
			ref={theme.rootRef}
		>
			<button
				className="wb-sessions-backdrop"
				type="button"
				tabIndex={sessionsOpen ? 0 : -1}
				aria-label={t("oqtoUi.mobile.closeSessions")}
				onClick={() => setSessionsOpen(false)}
			/>
			<div className="wb-content">
				<MobileTopBar
					directory={context.directory}
					session={context.session}
					activeView={navigation.mobileView}
					activeTab={navigation.workAreaTab}
					tabs={snapshot.workArea.tabs}
					taskProgress={
						<TaskProgress
							tasks={context.session.tasks ?? []}
							placement="mobile"
						/>
					}
					onOpenSessions={() => setSessionsOpen(true)}
					onNavigate={navigate}
				/>
				<CornerModeChrome
					config={config}
					directories={snapshot.workDirectories}
					directory={context.directory}
					session={context.session}
					onNavigate={navigate}
				>
					{mobile ? (
						<MobileCompositor
							store={store}
							mobileView={navigation.mobileView}
							renderContent={renderContent}
						/>
					) : (
						<CompositorHost
							store={store}
							viewport={viewport}
							renderContent={renderContent}
							contentLabel={contentLabel}
							labels={labels}
							keyBindings={keyBindings}
						/>
					)}
				</CornerModeChrome>
				<SessionStatusBar
					status={snapshot.environment.statusBar}
					session={context.session}
					models={snapshot.environment.models}
					settingsOpen={settingsOpen}
					onToggleSettings={toggleSettings}
				/>
			</div>
		</div>
	);
}
