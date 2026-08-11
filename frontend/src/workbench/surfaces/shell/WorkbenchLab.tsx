import { Activity, Users } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { workbenchLabFixture } from "../../modules/lab/fixture";
import type { LabNavigation } from "../../modules/lab/model";
import { useThemeRoot } from "../../modules/theme/useThemeRoot";
import type { WorkbenchUserTheme } from "../../modules/theme/userTheme";
import { ChatWorkspace } from "./ChatWorkspace";
import { FilesPane } from "./FilesPane";
import { MobileTopBar } from "./MobileTopBar";
import { NavigationRail } from "./NavigationRail";
import { SessionMeta } from "./SessionMeta";
import { ThemeCustomizer } from "./ThemeCustomizer";
import "./workbench-shell.css";

type WorkbenchLabProps = {
	workDirectoryId: string;
	sessionId: string;
	mobileView: string;
	schemeId: string;
	workAreaTab: string;
	onNavigate: (next: LabNavigation) => void;
};

export function WorkbenchLab({
	workDirectoryId,
	sessionId,
	mobileView,
	schemeId: requestedSchemeId,
	workAreaTab,
	onNavigate,
}: WorkbenchLabProps) {
	const { t } = useTranslation();
	const [sessionsOpen, setSessionsOpen] = useState(false);
	const [userTheme, setUserTheme] = useState<WorkbenchUserTheme>({});
	const { schemeId, rootRef, rootEl } = useThemeRoot(
		requestedSchemeId,
		userTheme,
	);
	const fixture = workbenchLabFixture;
	const directory =
		fixture.workDirectories.find((item) => item.id === workDirectoryId) ??
		fixture.workDirectories[0];
	const session =
		directory.sessions.find((item) => item.id === sessionId) ??
		directory.sessions[0];
	const status = fixture.statusBar;

	return (
		<div className="wb-shell" data-sessions-open={sessionsOpen} ref={rootRef}>
			<NavigationRail
				workDirectories={fixture.workDirectories}
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
				aria-label={t("workbench.mobile.closeSessions")}
				onClick={() => setSessionsOpen(false)}
			/>
			<div className="wb-content">
				<MobileTopBar
					directory={directory}
					session={session}
					activeView={mobileView}
					activeTab={workAreaTab}
					tabs={fixture.workArea.tabs}
					onOpenSessions={() => setSessionsOpen(true)}
					onNavigate={onNavigate}
				/>
				<div className="wb-workarea" data-view={mobileView}>
					<ChatWorkspace
						directory={directory}
						session={session}
						messages={fixture.messages}
						tasks={session.tasks ?? []}
						workArea={fixture.workArea}
						workAreaTab={workAreaTab}
						onNavigate={onNavigate}
					/>
					<FilesPane files={fixture.files} />
				</div>
				<ThemeCustomizer
					themeRoot={rootEl}
					userTheme={userTheme}
					onChange={setUserTheme}
				/>
				<footer className="wb-statusbar">
					<div className="wb-statusbar__group">
						<span
							className="wb-statusbar__item"
							title={t("workbench.statusBar.runningSessions")}
						>
							<Activity aria-hidden="true" />
							{status.runningSessions}
						</span>
						<SessionMeta
							key={session.id}
							session={session}
							models={fixture.models}
						/>
					</div>
					<div className="wb-statusbar__group">
						<span
							className="wb-statusbar__item"
							title={t("workbench.statusBar.onlineUsers")}
						>
							<Users aria-hidden="true" />
							{status.onlineUsers}
						</span>
						<span
							className="wb-statusbar__item"
							title={t("workbench.statusBar.runnerLoad")}
						>
							<Activity aria-hidden="true" />
							{status.runnerLoad}
						</span>
						<span
							className="wb-statusbar__item wb-statusbar__item--dim"
							title={t("workbench.statusBar.version")}
						>
							{status.version}
						</span>
					</div>
				</footer>
			</div>
		</div>
	);
}
