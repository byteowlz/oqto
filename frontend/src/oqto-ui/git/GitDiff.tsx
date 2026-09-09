/**
 * A patch, rendered as a diff rather than as text. The git channel returns
 * unified patches, which is exactly what this renderer takes, so the pane
 * hands over what it already has.
 *
 * The renderer is loaded on demand and never at module scope: it is a large
 * dependency behind one Content kind, and a shell that cannot load because
 * one pane's library is missing is a worse shell. Until it arrives — or if
 * it never does — the patch is shown as the text it already is.
 *
 * Split view needs room, so the layout follows the Container's width: a
 * narrow side pane stacks and wraps, a wide one shows both sides.
 */

import { useMountEffect } from "@/hooks/use-mount-effect";
import { type ComponentType, useState } from "react";
import { useTranslation } from "react-i18next";
import type { GitDiff as GitDiffValue } from "../platform/git-contract";

/** Below this the split view has no room for two readable columns. */
const SPLIT_FROM = 720;

interface PatchProps {
	readonly patch: string;
	readonly options: {
		readonly diffStyle: "unified" | "split";
		readonly overflow: "scroll" | "wrap";
		readonly theme: string;
		readonly stickyHeader: boolean;
	};
}

interface GitDiffProps {
	readonly diff: GitDiffValue;
	/** Measured width of the pane, in CSS pixels. */
	readonly width: number;
	/** The viewer's colour mode, so the patch wears the shell's own. */
	readonly dark: boolean;
}

export function GitDiff({ diff, width, dark }: GitDiffProps) {
	const { t } = useTranslation();
	const [renderer, setRenderer] = useState<ComponentType<PatchProps> | null>(
		null,
	);

	useMountEffect(() => {
		let live = true;
		void import("@pierre/diffs/react").then(
			(module) => {
				if (live)
					setRenderer(() => module.PatchDiff as ComponentType<PatchProps>);
			},
			() => {},
		);
		return () => {
			live = false;
		};
	});

	const Patch = renderer;
	return (
		<div className="wb-git__diff">
			{Patch ? (
				<Patch
					patch={diff.patch}
					options={{
						diffStyle: width >= SPLIT_FROM ? "split" : "unified",
						overflow: width >= SPLIT_FROM ? "scroll" : "wrap",
						theme: dark ? "vitesse-dark" : "vitesse-light",
						stickyHeader: true,
					}}
				/>
			) : (
				<pre className="wb-git__patch">{diff.patch}</pre>
			)}
			{diff.truncated ? (
				<p className="wb-git__note">{t("oqtoUi.git.diffTruncated")}</p>
			) : null}
		</div>
	);
}
