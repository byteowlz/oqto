/**
 * The classic OqtoUI preset composed through the ADR-0041 compositor: the
 * real NavigationRail, ChatWorkspace, and FilesPane become Container
 * Content rendered by CompositorHost. Layout persists device-locally via
 * the platform layout store; every change is a semantic transaction.
 */

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChatWorkspace } from "../chat/ChatWorkspace";
import type { PreviewSelection } from "../chat/ResourcePreviewPane";
import {
	type ContentRef,
	contentIdFrom,
	createClassicPresetLayout,
} from "../compositor/index";
import { CompositorHost } from "../compositor/react/CompositorHost";
import type {
	ContentLabel,
	RenderContent,
} from "../compositor/react/contracts";
import {
	type PersistedCompositorStore,
	createPersistedCompositorStore,
} from "../compositor/react/persisted-store";
import { useViewportClass } from "../compositor/react/useViewportClass";
import { FilesPane } from "../files/FilesPane";
import { GalleryPane } from "../gallery/GalleryPane";
import type {
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
import { NavigationRail } from "../sessions/NavigationRail";
import { useThemeRoot } from "../theme/useThemeRoot";
import { Splash } from "./Splash";
import "./shell.css";

type CompositorShellProps = {
	platform: OqtoUiPlatform;
	workDirectoryId: string | null;
	sessionId: string | null;
	schemeId: string | null;
	workAreaTab: string;
	storage: LayoutDocumentStore;
	onNavigate: (next: UiNavigation) => void;
};

const SESSIONS_CONTENT: ContentRef = {
	id: contentIdFrom("sessions:catalog"),
	kind: "sessions",
};

function chatContent(sessionId: string): ContentRef {
	return {
		id: contentIdFrom(`chat:${sessionId}`),
		kind: "chat",
		extensions: { sessionId },
	};
}

function filesContent(workDirectoryId: string): ContentRef {
	return {
		id: contentIdFrom(`files:${workDirectoryId}`),
		kind: "files",
		extensions: { workDirectoryId },
	};
}

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

function findSession(snapshot: OqtoUiSnapshot, sessionId: string) {
	for (const directory of snapshot.workDirectories) {
		const session = directory.sessions.find(
			(candidate) => candidate.id === sessionId,
		);
		if (session) return { directory, session };
	}
	return null;
}

export function CompositorShell({
	platform,
	workDirectoryId,
	sessionId,
	schemeId: requestedSchemeId,
	workAreaTab,
	storage,
	onNavigate,
}: CompositorShellProps) {
	const { t } = useTranslation();
	const { schemeId, rootRef } = useThemeRoot(
		requestedSchemeId ?? "oqto-dark",
		{},
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
		<div className="wb-shell" data-compositor="classic" ref={rootRef}>
			<LoadedCompositorShell
				snapshot={snapshot}
				platform={platform}
				context={{ directory, session }}
				workAreaTab={workAreaTab}
				storage={storage}
				schemeId={schemeId}
				onNavigate={onNavigate}
			/>
		</div>
	);
}

type LoadedCompositorShellProps = {
	snapshot: OqtoUiSnapshot;
	platform: OqtoUiPlatform;
	context: { directory: WorkDirectory; session: SessionOverview };
	workAreaTab: string;
	storage: LayoutDocumentStore;
	schemeId: string;
	onNavigate: (next: UiNavigation) => void;
};

function LoadedCompositorShell({
	snapshot,
	platform,
	context,
	workAreaTab,
	storage,
	schemeId,
	onNavigate,
}: LoadedCompositorShellProps) {
	const { t } = useTranslation();
	const viewport = useViewportClass();
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
	const navigate = useCallback(
		(next: UiNavigation) => {
			if (next.sessionId) ensureSessionChat(store, next.sessionId);
			onNavigate(next);
		},
		[store, onNavigate],
	);
	const previewState = useMemo(
		() => ({
			selection: preview,
			open: setPreview,
			close: () => setPreview(null),
		}),
		[preview],
	);
	const renderContent: RenderContent = useCallback(
		(content) => {
			if (content.kind === "sessions") {
				return (
					<NavigationRail
						workDirectories={snapshot.workDirectories}
						workDirectoryId={context.directory.id}
						sessionId={context.session.id}
						schemeId={schemeId}
						onNavigate={navigate}
					/>
				);
			}
			if (content.kind === "chat") {
				const sessionId = content.extensions?.sessionId;
				const target =
					typeof sessionId === "string"
						? findSession(snapshot, sessionId)
						: null;
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
					/>
				);
			}
			if (content.kind === "files") return <FilesPane files={snapshot.files} />;
			return (
				<div className="oqto-compositor-unavailable" data-kind={content.kind} />
			);
		},
		[
			snapshot,
			platform,
			context,
			workAreaTab,
			schemeId,
			navigate,
			previewState,
		],
	);
	const contentLabel: ContentLabel = useCallback(
		(content) => {
			if (content.kind === "sessions") return t("oqtoUi.navigation.sessions");
			if (content.kind === "files") return t("oqtoUi.files.label");
			if (content.kind === "chat") {
				const sessionId = content.extensions?.sessionId;
				const target =
					typeof sessionId === "string"
						? findSession(snapshot, sessionId)
						: null;
				return target?.session.name ?? content.id;
			}
			return content.kind;
		},
		[snapshot, t],
	);
	const labels = useMemo(
		() => ({
			closeTab: t("common.close"),
			resizeColumns: t("oqtoUi.compositor.resizeColumns"),
			resizeRows: t("oqtoUi.compositor.resizeRows"),
			dropTop: t("oqtoUi.compositor.dropTop"),
			dropBottom: t("oqtoUi.compositor.dropBottom"),
			dropStart: t("oqtoUi.compositor.dropStart"),
			dropEnd: t("oqtoUi.compositor.dropEnd"),
		}),
		[t],
	);
	return (
		<CompositorHost
			store={store}
			viewport={viewport}
			renderContent={renderContent}
			contentLabel={contentLabel}
			labels={labels}
		/>
	);
}
