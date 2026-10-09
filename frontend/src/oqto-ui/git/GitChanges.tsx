/**
 * One list of changed paths — staged or unstaged — with the status letter
 * git itself reports and the one control that moves a path across.
 */

import { useTranslation } from "react-i18next";
import type { GitEntry } from "../platform/git-contract";

interface GitChangesProps {
	readonly heading: string;
	readonly entries: readonly GitEntry[];
	/** Which side this list shows; the control moves paths the other way. */
	readonly staged: boolean;
	readonly onShow: (path: string, staged: boolean) => void;
	readonly onStage: (paths: readonly string[], staged: boolean) => void;
}

export function GitChanges({
	heading,
	entries,
	staged,
	onShow,
	onStage,
}: GitChangesProps) {
	const { t } = useTranslation();
	if (entries.length === 0) return null;
	return (
		<section className="wb-git__group">
			<h3>
				{heading}
				<button
					type="button"
					onClick={() =>
						onStage(
							entries.map((entry) => entry.path),
							!staged,
						)
					}
				>
					{t(staged ? "oqtoUi.git.unstageAll" : "oqtoUi.git.stageAll")}
				</button>
			</h3>
			<ul>
				{entries.map((entry) => (
					<li key={`${staged}-${entry.path}`}>
						<span className="wb-git__code" data-code={code(entry, staged)}>
							{code(entry, staged)}
						</span>
						<button
							type="button"
							className="wb-git__path"
							onClick={() => onShow(entry.path, staged)}
						>
							{entry.renamedFrom
								? `${entry.renamedFrom} → ${entry.path}`
								: entry.path}
						</button>
						<button
							type="button"
							className="wb-git__move"
							aria-label={t(
								staged ? "oqtoUi.git.unstage" : "oqtoUi.git.stage",
								{
									path: entry.path,
								},
							)}
							onClick={() => onStage([entry.path], !staged)}
						>
							{staged ? "−" : "+"}
						</button>
					</li>
				))}
			</ul>
		</section>
	);
}

function code(entry: GitEntry, staged: boolean): string {
	const letter = staged ? entry.index : entry.worktree;
	return letter.trim() === "" ? "·" : letter;
}
