/**
 * The branch and the remote: which branch the tree is on, which others it
 * could move to, and the three operations that talk to the remote. A branch
 * another checkout already holds is offered but marked, because switching to
 * it is what git will refuse.
 */

import { ArrowDown, ArrowUp, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { GitBranch, RemoteOutcome } from "../platform/git-contract";

interface GitBranchBarProps {
	readonly branches: readonly GitBranch[];
	readonly current: string;
	readonly onSwitch: (branch: string) => void;
	readonly onRemote: (operation: RemoteOutcome["operation"]) => void;
}

export function GitBranchBar({
	branches,
	current,
	onSwitch,
	onRemote,
}: GitBranchBarProps) {
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);
	const others = branches.filter((branch) => branch.name !== current);
	return (
		<>
			<button
				type="button"
				className="wb-git__branch"
				aria-expanded={open}
				aria-label={t("oqtoUi.git.branches")}
				disabled={others.length === 0}
				onClick={() => setOpen(!open)}
			>
				{current || t("oqtoUi.files.loading")}
			</button>
			{open ? (
				<ul className="wb-git__branches">
					{others.map((branch) => (
						<li key={branch.name}>
							<button
								type="button"
								onClick={() => {
									setOpen(false);
									onSwitch(branch.name);
								}}
							>
								{branch.name}
								{branch.worktree ? (
									<small>{t("oqtoUi.git.inWorktree")}</small>
								) : null}
							</button>
						</li>
					))}
				</ul>
			) : null}
			<button
				type="button"
				className="wb-icon-button"
				aria-label={t("oqtoUi.git.fetch")}
				title={t("oqtoUi.git.fetch")}
				onClick={() => onRemote("fetch")}
			>
				<RefreshCw aria-hidden="true" />
			</button>
			<button
				type="button"
				className="wb-icon-button"
				aria-label={t("oqtoUi.git.pull")}
				title={t("oqtoUi.git.pull")}
				onClick={() => onRemote("pull")}
			>
				<ArrowDown aria-hidden="true" />
			</button>
			<button
				type="button"
				className="wb-icon-button"
				aria-label={t("oqtoUi.git.push")}
				title={t("oqtoUi.git.push")}
				onClick={() => onRemote("push")}
			>
				<ArrowUp aria-hidden="true" />
			</button>
		</>
	);
}
