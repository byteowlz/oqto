import { Activity, SlidersHorizontal, Users } from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
	ModelOption,
	SessionOverview,
	StatusBarData,
} from "../platform/contracts";
import { SessionMeta } from "./SessionMeta";

type SessionStatusBarProps = {
	status: StatusBarData | null;
	session: SessionOverview;
	models: ModelOption[];
	settingsOpen: boolean;
	onToggleSettings: () => void;
};

export function SessionStatusBar({
	status,
	session,
	models,
	settingsOpen,
	onToggleSettings,
}: SessionStatusBarProps) {
	const { t } = useTranslation();
	return (
		<footer className="wb-statusbar">
			<div className="wb-statusbar__group">
				{status && status.runningSessions ? (
					<span
						className="wb-statusbar__item"
						title={t("oqtoUi.statusBar.runningSessions")}
					>
						<Activity aria-hidden="true" />
						{status.runningSessions}
					</span>
				) : null}
				<SessionMeta key={session.id} session={session} models={models} />
			</div>
			<div className="wb-statusbar__group">
				{status ? (
					<>
						{status.onlineUsers ? (
							<span
								className="wb-statusbar__item"
								title={t("oqtoUi.statusBar.onlineUsers")}
							>
								<Users aria-hidden="true" />
								{status.onlineUsers}
							</span>
						) : null}
						{status.runnerLoad ? (
							<span
								className="wb-statusbar__item"
								title={t("oqtoUi.statusBar.runnerLoad")}
							>
								<Activity aria-hidden="true" />
								{status.runnerLoad}
							</span>
						) : null}
						{status.version ? (
							<span
								className="wb-statusbar__item wb-statusbar__item--dim"
								title={t("oqtoUi.statusBar.version")}
							>
								{status.version}
							</span>
						) : null}
					</>
				) : null}
				<button
					className="wb-statusbar__button"
					type="button"
					aria-expanded={settingsOpen}
					aria-label={t("oqtoUi.settings.open")}
					onClick={onToggleSettings}
				>
					<SlidersHorizontal aria-hidden="true" />
				</button>
			</div>
		</footer>
	);
}
