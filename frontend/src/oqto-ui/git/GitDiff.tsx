/**
 * A patch, rendered as a diff rather than as text. The git channel returns
 * unified patches, which is exactly what this renderer takes, so the pane
 * hands over what it already has.
 *
 * Split view needs room, so the layout follows the Container's width: a
 * narrow side pane stacks and wraps, a wide one shows both sides.
 */

import { PatchDiff } from "@pierre/diffs/react";
import { useTranslation } from "react-i18next";
import type { GitDiff as GitDiffValue } from "../platform/git-contract";

/** Below this the split view has no room for two readable columns. */
const SPLIT_FROM = 720;

interface GitDiffProps {
	readonly diff: GitDiffValue;
	/** Measured width of the pane, in CSS pixels. */
	readonly width: number;
	/** The viewer's colour mode, so the patch wears the shell's own. */
	readonly dark: boolean;
}

export function GitDiff({ diff, width, dark }: GitDiffProps) {
	const { t } = useTranslation();
	return (
		<div className="wb-git__diff">
			<PatchDiff
				patch={diff.patch}
				options={{
					diffStyle: width >= SPLIT_FROM ? "split" : "unified",
					overflow: width >= SPLIT_FROM ? "scroll" : "wrap",
					theme: dark ? "vitesse-dark" : "vitesse-light",
					stickyHeader: true,
				}}
			/>
			{diff.truncated ? (
				<p className="wb-git__note">{t("oqtoUi.git.diffTruncated")}</p>
			) : null}
		</div>
	);
}
