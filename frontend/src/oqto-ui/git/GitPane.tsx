/**
 * The work directory's version control as placeable Content: the branch and
 * its distance from upstream, staged and unstaged changes as two lists, the
 * selected path's diff, and a commit line. Git's two status codes stay
 * distinct, so a path changed in both places appears in both lists.
 */

import { RefreshCw } from "lucide-react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import type { GitEntry, GitHost } from "../platform/git-contract";
import { GitChanges } from "./GitChanges";
import { useGitStatus } from "./useGitStatus";

interface GitPaneProps {
	readonly gitHost: GitHost;
	/** Host path of the work directory this repository lives in. */
	readonly workspacePath: string;
	/** The Container's own controls, placed in this pane's bar. */
	readonly chrome?: ReactNode;
}

/** Entries with anything in the index; a rename counts as staged. */
export function staged(entries: readonly GitEntry[]): readonly GitEntry[] {
	return entries.filter((entry) => entry.index !== " " && entry.index !== "?");
}

/** Entries the working tree changed, including untracked ones. */
export function unstaged(entries: readonly GitEntry[]): readonly GitEntry[] {
	return entries.filter((entry) => entry.worktree !== " ");
}

export function GitPane({ gitHost, workspacePath, chrome }: GitPaneProps) {
	const { t } = useTranslation();
	const git = useGitStatus(gitHost, workspacePath);
	const [message, setMessage] = useState("");
	const entries = git.status?.entries ?? [];
	const stagedEntries = staged(entries);
	const distance = git.status
		? [
				git.status.ahead > 0
					? t("oqtoUi.git.ahead", { count: git.status.ahead })
					: "",
				git.status.behind > 0
					? t("oqtoUi.git.behind", { count: git.status.behind })
					: "",
			]
				.filter(Boolean)
				.join(" ")
		: "";
	return (
		<section className="wb-git" aria-label={t("oqtoUi.git.label")}>
			<header className="wb-git__bar">
				<span className="wb-git__branch">
					{git.status?.branch ?? t("oqtoUi.files.loading")}
				</span>
				{distance ? <small>{distance}</small> : null}
				<button
					type="button"
					className="wb-icon-button"
					aria-label={t("oqtoUi.git.reload")}
					title={t("oqtoUi.git.reload")}
					onClick={git.reload}
				>
					<RefreshCw aria-hidden="true" />
				</button>
				{chrome}
			</header>
			<div className="wb-git__body">
				{git.failed ? (
					<p className="wb-git__note">{git.failed}</p>
				) : entries.length === 0 && !git.loading ? (
					<p className="wb-git__note">{t("oqtoUi.git.clean")}</p>
				) : (
					<>
						<GitChanges
							heading={t("oqtoUi.git.staged")}
							entries={stagedEntries}
							staged
							onShow={git.show}
							onStage={git.stage}
						/>
						<GitChanges
							heading={t("oqtoUi.git.changed")}
							entries={unstaged(entries)}
							staged={false}
							onShow={git.show}
							onStage={git.stage}
						/>
					</>
				)}
				{git.status?.truncated ? (
					<p className="wb-git__note">{t("oqtoUi.git.truncated")}</p>
				) : null}
				{git.diff ? <pre className="wb-git__diff">{git.diff.patch}</pre> : null}
			</div>
			<footer className="wb-git__commit">
				<input
					type="text"
					value={message}
					placeholder={t("oqtoUi.git.message")}
					aria-label={t("oqtoUi.git.message")}
					onChange={(event) => setMessage(event.target.value)}
					onKeyDown={(event) => {
						if (event.key !== "Enter" || message.trim() === "") return;
						event.preventDefault();
						git.commit(message.trim());
						setMessage("");
					}}
				/>
				<button
					type="button"
					disabled={stagedEntries.length === 0 || message.trim() === ""}
					onClick={() => {
						git.commit(message.trim());
						setMessage("");
					}}
				>
					{t("oqtoUi.git.commit", { count: stagedEntries.length })}
				</button>
			</footer>
		</section>
	);
}
