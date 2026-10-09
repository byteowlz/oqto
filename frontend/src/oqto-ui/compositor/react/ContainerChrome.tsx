/**
 * A Container's own controls: add Content, and close the Container by
 * closing everything it holds. The Container hands these to its Content to
 * place inside the presentation's own header, so a Container holding one
 * Content needs no strip of its own; only a Container with several Contents
 * grows a tab bar to hold them.
 */

import type { ContainerId, ContentRef } from "../index";
import { AddContent } from "./AddContent";
import type {
	CommitCommands,
	CompositorChromeLabels,
	ContentServices,
} from "./contracts";

interface ContainerChromeProps {
	readonly containerId: ContainerId;
	readonly stack: readonly ContentRef[];
	readonly content: ContentServices;
	readonly labels: CompositorChromeLabels;
	readonly commit: CommitCommands;
}

export function ContainerChrome({
	containerId,
	stack,
	content,
	labels,
	commit,
}: ContainerChromeProps) {
	return (
		<div className="oqto-compositor-chrome">
			<AddContent
				options={content.addable}
				label={content.label}
				labels={labels.add}
				onAdd={(added, placement) => content.add(added, containerId, placement)}
			/>
			{stack.length > 0 ? (
				<button
					type="button"
					className="oqto-compositor-close"
					aria-label={labels.closeContainer}
					title={labels.closeContainer}
					onClick={() =>
						commit(
							stack.map((item) => ({
								type: "close" as const,
								contentId: item.id,
							})),
						)
					}
				>
					{"×"}
				</button>
			) : null}
		</div>
	);
}
