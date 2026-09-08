/**
 * The add affordance on a Container's four edges. Each control opens a short
 * list of the Content the host offers; choosing one places a new Container in
 * that direction. Revealed on hover or keyboard focus by the stylesheet, so
 * an idle Arrangement stays quiet.
 */

import { useState } from "react";
import type { ContentRef, SplitEdge } from "../index";
import type { AddLabels, ContentLabel } from "./contracts";

const EDGES: readonly SplitEdge[] = [
	"inline-start",
	"inline-end",
	"block-start",
	"block-end",
];

interface AddContainerProps {
	readonly options: readonly ContentRef[];
	readonly label: ContentLabel;
	readonly labels: AddLabels;
	readonly onAdd: (content: ContentRef, edge: SplitEdge) => void;
}

export function AddContainer({
	options,
	label,
	labels,
	onAdd,
}: AddContainerProps) {
	const [open, setOpen] = useState<SplitEdge | null>(null);
	if (options.length === 0) return null;
	return (
		<>
			{EDGES.map((edge) => (
				<div className="oqto-compositor-add" data-edge={edge} key={edge}>
					<button
						type="button"
						className="oqto-compositor-add-open"
						aria-label={labels.edges[edge]}
						title={labels.edges[edge]}
						aria-expanded={open === edge}
						onClick={() => setOpen(open === edge ? null : edge)}
					>
						+
					</button>
					{open === edge ? (
						<ul
							className="oqto-compositor-add-menu"
							aria-label={labels.edges[edge]}
							onKeyDown={(event) => {
								if (event.key === "Escape") setOpen(null);
							}}
						>
							{options.map((content) => (
								<li key={content.id}>
									<button
										type="button"
										onClick={() => {
											setOpen(null);
											onAdd(content, edge);
										}}
									>
										{label(content)}
									</button>
								</li>
							))}
						</ul>
					) : null}
				</div>
			))}
		</>
	);
}
