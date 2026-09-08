/**
 * The work directory's issue tracker as placeable Content: issues grouped
 * by status in tracker order, each row showing what it is blocked by, with
 * a control to move it to the next state.
 */

import { CircleCheck, CirclePlay, Eye, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Issue, IssueStatus } from "../platform/issues-contract";
import type { IssueHost } from "../platform/issues-contract";
import { useIssues } from "./useIssues";

const GROUPS: readonly IssueStatus[] = [
	"in_progress",
	"blocked",
	"ready",
	"open",
	"closed",
];

/**
 * A real tracker holds thousands of issues, nearly all of them closed. The
 * pane shows the working set: closed issues are opt-in, and every group is
 * capped so no listing can swamp the Container.
 */
const VISIBLE_PER_GROUP = 50;

interface IssuesPaneProps {
	readonly issueHost: IssueHost;
	/** Host path of the work directory whose tracker this is. */
	readonly workspacePath: string;
}

function byPriority(left: Issue, right: Issue): number {
	return left.priority - right.priority || left.id.localeCompare(right.id);
}

export function IssuesPane({ issueHost, workspacePath }: IssuesPaneProps) {
	const { t } = useTranslation();
	const board = useIssues(issueHost, workspacePath);
	const [showClosed, setShowClosed] = useState(false);
	const groups = GROUPS.filter((status) => status !== "closed" || showClosed)
		.map((status) => ({
			status,
			issues: board.issues
				.filter((issue) => issue.status === status)
				.sort(byPriority),
		}))
		.filter((group) => group.issues.length > 0);
	return (
		<section className="wb-issues" aria-label={t("oqtoUi.issues.label")}>
			<header className="wb-issues__bar">
				<span>{t("oqtoUi.issues.count", { count: board.issues.length })}</span>
				<button
					type="button"
					className="wb-icon-button"
					aria-label={t("oqtoUi.issues.showClosed")}
					title={t("oqtoUi.issues.showClosed")}
					aria-pressed={showClosed}
					onClick={() => setShowClosed(!showClosed)}
				>
					<Eye aria-hidden="true" />
				</button>
				<button
					type="button"
					className="wb-icon-button"
					aria-label={t("oqtoUi.issues.reload")}
					title={t("oqtoUi.issues.reload")}
					onClick={board.reload}
				>
					<RefreshCw aria-hidden="true" />
				</button>
			</header>
			<div className="wb-issues__body">
				{board.failed ? (
					<p className="wb-issues__note">{t("oqtoUi.issues.failed")}</p>
				) : board.loading && board.issues.length === 0 ? (
					<p className="wb-issues__note">{t("oqtoUi.files.loading")}</p>
				) : groups.length === 0 ? (
					<p className="wb-issues__note">{t("oqtoUi.issues.empty")}</p>
				) : (
					groups.map((group) => (
						<section key={group.status} className="wb-issues__group">
							<h3>{t(`oqtoUi.issues.status.${group.status}`)}</h3>
							<ul>
								{group.issues.slice(0, VISIBLE_PER_GROUP).map((issue) => (
									<li key={issue.id} data-status={issue.status}>
										<span className="wb-issues__id">{issue.id}</span>
										<span className="wb-issues__title">{issue.title}</span>
										{issue.blockedBy.length > 0 ? (
											<small className="wb-issues__blocked">
												{t("oqtoUi.issues.blockedBy", {
													ids: issue.blockedBy.join(", "),
												})}
											</small>
										) : null}
										{issue.status === "closed" ? null : (
											<button
												type="button"
												className="wb-icon-button"
												aria-label={t(
													issue.status === "in_progress"
														? "oqtoUi.issues.close"
														: "oqtoUi.issues.start",
													{ id: issue.id },
												)}
												onClick={() =>
													board.move(
														issue.id,
														issue.status === "in_progress"
															? "closed"
															: "in_progress",
													)
												}
											>
												{issue.status === "in_progress" ? (
													<CircleCheck aria-hidden="true" />
												) : (
													<CirclePlay aria-hidden="true" />
												)}
											</button>
										)}
									</li>
								))}
								{group.issues.length > VISIBLE_PER_GROUP ? (
									<li className="wb-issues__more">
										{t("oqtoUi.issues.more", {
											count: group.issues.length - VISIBLE_PER_GROUP,
										})}
									</li>
								) : null}
							</ul>
						</section>
					))
				)}
			</div>
		</section>
	);
}
