/**
 * One Container's chrome: tab strip (hidden for a single item) plus the
 * active Content presentation. Memoized on the Container's object identity
 * — the kernel's structural sharing keeps unchanged Containers identical,
 * so unrelated Containers never re-render.
 */

import { memo } from "react";
import type { Container, ContentId } from "../index";
import type {
	CommitCommands,
	CompositorChromeLabels,
	ContentLabel,
	RenderContent,
} from "./contracts";

interface ContainerViewProps {
	readonly container: Container;
	/** Focused Content id when it lives in this Container, else null. */
	readonly focusedContentId: ContentId | null;
	readonly renderContent: RenderContent;
	readonly contentLabel: ContentLabel;
	readonly labels: CompositorChromeLabels;
	readonly commit: CommitCommands;
}

export const ContainerView = memo(function ContainerView({
	container,
	focusedContentId,
	renderContent,
	contentLabel,
	labels,
	commit,
}: ContainerViewProps) {
	const active =
		container.stack.find(
			(content) => content.id === container.activeContentId,
		) ?? null;
	return (
		<section
			className="oqto-compositor-container"
			data-container-id={container.id}
			data-role={container.role}
			hidden={container.collapsed}
		>
			{container.stack.length > 1 ? (
				<div className="oqto-compositor-tabs" role="tablist">
					{container.stack.map((content) => (
						<div
							key={content.id}
							className="oqto-compositor-tab"
							data-active={
								content.id === container.activeContentId || undefined
							}
						>
							<button
								type="button"
								role="tab"
								aria-selected={content.id === container.activeContentId}
								onClick={() =>
									commit([{ type: "activate", contentId: content.id }])
								}
							>
								{contentLabel(content)}
							</button>
							<button
								type="button"
								className="oqto-compositor-tab-close"
								aria-label={labels.closeTab}
								onClick={() =>
									commit([{ type: "close", contentId: content.id }])
								}
							>
								{"×"}
							</button>
						</div>
					))}
				</div>
			) : null}
			<div
				className="oqto-compositor-content"
				onFocusCapture={() => {
					if (active && focusedContentId !== active.id) {
						commit([{ type: "focus", contentId: active.id }]);
					}
				}}
			>
				{active ? (
					renderContent(active, {
						containerId: container.id,
						active: true,
						focused: focusedContentId === active.id,
					})
				) : (
					<div className="oqto-compositor-empty" />
				)}
			</div>
		</section>
	);
});
