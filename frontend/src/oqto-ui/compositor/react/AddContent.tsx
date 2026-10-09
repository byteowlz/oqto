/**
 * One persistent add control per Container. It sits in the tab strip, so
 * the Arrangement carries a single quiet affordance instead of hover chrome
 * on every edge. The menu picks what to add and where: a tab in this
 * Container by default, or a new Container on one of its four sides.
 */

import { useState } from "react";
import type { ContentRef, SplitEdge } from "../index";
import type { AddLabels, ContentLabel } from "./contracts";

/** A tab in this Container, or a new Container on that edge. */
export type AddPlacement = "tab" | SplitEdge;

const PLACEMENTS: readonly AddPlacement[] = [
	"tab",
	"inline-start",
	"inline-end",
	"block-start",
	"block-end",
];

const GLYPHS: { readonly [placement in AddPlacement]: string } = {
	tab: "▤",
	"inline-start": "◧",
	"inline-end": "◨",
	"block-start": "⬒",
	"block-end": "⬓",
};

interface AddContentProps {
	readonly options: readonly ContentRef[];
	readonly label: ContentLabel;
	readonly labels: AddLabels;
	readonly onAdd: (content: ContentRef, placement: AddPlacement) => void;
}

export function AddContent({ options, label, labels, onAdd }: AddContentProps) {
	const [open, setOpen] = useState(false);
	const [placement, setPlacement] = useState<AddPlacement>("tab");
	if (options.length === 0) return null;
	return (
		<div
			className="oqto-compositor-add"
			onKeyDown={(event) => {
				if (event.key === "Escape") setOpen(false);
			}}
		>
			<button
				type="button"
				className="oqto-compositor-add-open"
				aria-label={labels.open}
				title={labels.open}
				aria-expanded={open}
				onClick={() => setOpen(!open)}
			>
				+
			</button>
			{open ? (
				<div className="oqto-compositor-add-menu">
					<div className="oqto-compositor-add-where" role="group">
						{PLACEMENTS.map((candidate) => (
							<button
								key={candidate}
								type="button"
								aria-label={labels.placements[candidate]}
								title={labels.placements[candidate]}
								aria-pressed={placement === candidate}
								onClick={() => setPlacement(candidate)}
							>
								{GLYPHS[candidate]}
							</button>
						))}
					</div>
					<ul aria-label={labels.placements[placement]}>
						{options.map((content) => (
							<li key={content.id}>
								<button
									type="button"
									onClick={() => {
										setOpen(false);
										onAdd(content, placement);
									}}
								>
									{label(content)}
								</button>
							</li>
						))}
					</ul>
				</div>
			) : null}
		</div>
	);
}
