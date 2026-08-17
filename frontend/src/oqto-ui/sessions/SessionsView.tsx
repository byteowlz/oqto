import { useTranslation } from "react-i18next";
import type { SessionSummary } from "../platform/contracts";

type SessionsViewProps = {
	sessions: SessionSummary[];
	activeSessionId: string | null;
	toSession: (sessionId: string) => string;
};

export function SessionsView({
	sessions,
	activeSessionId,
	toSession,
}: SessionsViewProps) {
	const { t } = useTranslation();
	if (sessions.length === 0) {
		return <p className="oui-empty">{t("oqtoUi.sessions.empty")}</p>;
	}
	return (
		<nav aria-label={t("oqtoUi.sessions.label")} className="oui-session-list">
			{sessions.map((session) => (
				<a
					key={session.id}
					href={toSession(session.id)}
					aria-current={session.id === activeSessionId ? "page" : undefined}
					className="oui-session-link"
				>
					<strong>{session.title}</strong>
					<span>{session.workspace}</span>
				</a>
			))}
		</nav>
	);
}
