/**
 * Content renderer registry for the compositor shell: maps Content kinds
 * (sessions, chat, files, settings) to the real OqtoUI panes. The
 * compositor never knows what a "chat" is; this is the composition edge.
 */

import type { TFunction } from "i18next";
import { ChatWorkspace } from "../chat/ChatWorkspace";
import type { PreviewState } from "../chat/ChatWorkspace";
import type {
	ContentLabel,
	RenderContent,
} from "../compositor/react/contracts";
import { FilePane } from "../files/FilePane";
import { WorkDirectoryFiles } from "../files/WorkDirectoryFiles";
import { GalleryPane } from "../gallery/GalleryPane";
import type {
	OqtoUiConfigResolution,
	OqtoUiPlatform,
	OqtoUiSnapshot,
	SessionOverview,
	UiNavigation,
	WorkDirectory,
} from "../platform/contracts";
import { NavigationRail } from "../sessions/NavigationRail";
import { SessionStatusBar } from "../sessions/SessionStatusBar";
import { TodosPane } from "../sessions/TodosPane";
import { SettingsPane } from "../theme/SettingsPane";
import type { OqtoUiUserTheme } from "../theme/userTheme";
import { findSession } from "./compositorRefs";

export interface ContentRendererDeps {
	readonly snapshot: OqtoUiSnapshot;
	readonly platform: OqtoUiPlatform;
	readonly context: {
		readonly directory: WorkDirectory;
		readonly session: SessionOverview;
	};
	readonly workAreaTab: string;
	readonly theme: {
		schemeId: string;
		rootEl: HTMLElement | null;
		userTheme: OqtoUiUserTheme;
		onUserTheme: (next: OqtoUiUserTheme) => void;
	};
	readonly resolvedConfig: OqtoUiConfigResolution;
	readonly previewState: PreviewState;
	readonly navigate: (next: UiNavigation) => void;
	readonly settings: {
		readonly open: boolean;
		readonly toggle: () => void;
		readonly close: () => void;
	};
	readonly chrome: {
		readonly collapseNavigation: () => void;
		readonly toggleAuxiliary: () => void;
		/** Opens a file as its own Content. */
		readonly openFile: (path: string) => void;
	};
}

export function createContentRenderer(
	deps: ContentRendererDeps,
): RenderContent {
	const {
		snapshot,
		platform,
		context,
		workAreaTab,
		theme,
		resolvedConfig,
		previewState,
		navigate,
		settings,
		chrome,
	} = deps;
	return (content) => {
		if (content.kind === "sessions") {
			return (
				<NavigationRail
					runnerTargets={platform.runnerTargets}
					workDirectories={snapshot.workDirectories}
					workDirectoryId={context.directory.id}
					sessionId={context.session.id}
					schemeId={theme.schemeId}
					onNavigate={navigate}
					onCollapse={chrome.collapseNavigation}
				/>
			);
		}
		if (content.kind === "chat") {
			const sessionId = content.extensions?.sessionId;
			const target =
				typeof sessionId === "string" ? findSession(snapshot, sessionId) : null;
			if (!target)
				return (
					<div
						className="oqto-compositor-unavailable"
						data-kind={content.kind}
					/>
				);
			return (
				<ChatWorkspace
					platform={platform}
					context={{
						directory: target.directory,
						session: target.session,
						tasks: target.session.tasks ?? [],
					}}
					workArea={snapshot.workArea}
					workAreaTab={workAreaTab}
					galleryPane={<GalleryPane resources={snapshot.gallery} />}
					previewState={previewState}
					onNavigate={navigate}
					onTogglePanel={chrome.toggleAuxiliary}
				/>
			);
		}
		if (content.kind === "files") {
			return (
				<WorkDirectoryFiles
					fileHost={platform.files}
					workspacePath={context.directory.path}
					onOpenFile={chrome.openFile}
				/>
			);
		}
		if (content.kind === "file") {
			const path = content.extensions?.path;
			if (typeof path !== "string")
				return (
					<div
						className="oqto-compositor-unavailable"
						data-kind={content.kind}
					/>
				);
			return (
				// Keyed by path: activating another file must remount the pane, or
				// the mount-time load leaves the previous file's text on screen.
				<FilePane
					key={path}
					fileHost={platform.files}
					workspacePath={context.directory.path}
					path={path}
				/>
			);
		}
		if (content.kind === "todos") {
			return <TodosPane tasks={context.session.tasks ?? []} />;
		}
		if (content.kind === "gallery") {
			return <GalleryPane resources={snapshot.gallery} />;
		}
		if (content.kind === "status") {
			return (
				<SessionStatusBar
					status={snapshot.environment.statusBar}
					session={context.session}
					models={snapshot.environment.models}
					settingsOpen={settings.open}
					onToggleSettings={settings.toggle}
				/>
			);
		}
		if (content.kind === "settings") {
			return (
				<SettingsPane
					schemeId={theme.schemeId}
					themeRoot={theme.rootEl}
					userTheme={theme.userTheme}
					resolution={resolvedConfig}
					onChange={theme.onUserTheme}
					onNavigate={navigate}
					onClose={settings.close}
				/>
			);
		}
		return (
			<div className="oqto-compositor-unavailable" data-kind={content.kind} />
		);
	};
}

export function createContentLabel(
	snapshot: OqtoUiSnapshot,
	t: TFunction,
): ContentLabel {
	return (content) => {
		if (content.kind === "sessions") return t("oqtoUi.navigation.sessions");
		if (content.kind === "files") return t("oqtoUi.files.label");
		if (content.kind === "settings") return t("oqtoUi.settings.label");
		if (content.kind === "todos") return t("oqtoUi.todos.label");
		if (content.kind === "file") {
			const path = content.extensions?.path;
			return typeof path === "string"
				? (path.split("/").pop() ?? path)
				: t("oqtoUi.files.label");
		}
		if (content.kind === "gallery") return t("oqtoUi.gallery.label");
		if (content.kind === "status")
			return t("oqtoUi.statusBar.label", { defaultValue: "Status" });
		if (content.kind === "chat") {
			const sessionId = content.extensions?.sessionId;
			const target =
				typeof sessionId === "string" ? findSession(snapshot, sessionId) : null;
			return target?.session.name ?? content.id;
		}
		return content.kind;
	};
}
