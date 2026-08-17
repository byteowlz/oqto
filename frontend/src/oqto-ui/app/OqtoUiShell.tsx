import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { ChatView } from "../chat/ChatView";
import { GalleryView } from "../gallery/GalleryView";
import { ViewFrame } from "../layout/ViewFrame";
import type { OqtoUiPlatform } from "../platform/contracts";
import { SessionsView } from "../sessions/SessionsView";
import "./oqto-ui.css";

type OqtoUiShellProps = {
	platform: OqtoUiPlatform;
	mode: "live" | "scripted";
};

export function OqtoUiShell({ platform, mode }: OqtoUiShellProps) {
	const { t } = useTranslation();
	const [searchParams] = useSearchParams();
	const selectedSessionId = searchParams.get("session");
	const snapshot = useQuery({
		queryKey: ["oqto-ui", mode, selectedSessionId],
		queryFn: () => platform.load(selectedSessionId),
		staleTime: 10_000,
	});

	if (snapshot.isPending) {
		return <main className="oui-state">{t("oqtoUi.loading")}</main>;
	}
	if (snapshot.isError) {
		return (
			<main className="oui-state" role="alert">
				<h1>{t("oqtoUi.error.title")}</h1>
				<p>{t("oqtoUi.error.body")}</p>
				<button type="button" onClick={() => snapshot.refetch()}>
					{t("oqtoUi.error.retry")}
				</button>
			</main>
		);
	}

	const data = snapshot.data;
	const route = mode === "scripted" ? "/dev/oqto-ui" : "/oqto-ui";
	const active = data.sessions.find(
		(session) => session.id === data.activeSessionId,
	);
	return (
		<main className="oui-shell" data-mode={mode}>
			<header className="oui-topbar">
				<a href={route} className="oui-brand" translate="no">
					{t("oqtoUi.brand")}
				</a>
				<div>
					<strong>{active?.title ?? t("oqtoUi.noSession")}</strong>
					<span>{active?.workspace ?? t("oqtoUi.noWorkspace")}</span>
				</div>
				<output data-connection={data.connection}>
					{t(`oqtoUi.connection.${data.connection}`)}
				</output>
			</header>
			<div className="oui-layout">
				<ViewFrame
					viewId="sessions"
					title={t("oqtoUi.views.sessions")}
					meta={t("oqtoUi.fidelity.compact")}
				>
					<SessionsView
						sessions={data.sessions}
						activeSessionId={data.activeSessionId}
						toSession={(sessionId) =>
							`${route}?session=${encodeURIComponent(sessionId)}`
						}
					/>
				</ViewFrame>
				<ViewFrame
					viewId="chat"
					title={t("oqtoUi.views.chat")}
					meta={t("oqtoUi.fidelity.expanded")}
				>
					<ChatView entries={data.timeline} />
				</ViewFrame>
				<ViewFrame
					viewId="gallery"
					title={t("oqtoUi.views.gallery")}
					meta={t("oqtoUi.fidelity.standard")}
				>
					<GalleryView resources={data.gallery} />
				</ViewFrame>
			</div>
		</main>
	);
}
