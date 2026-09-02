/**
 * One Container's chrome: tab strip (hidden for a single item) plus the
 * active Content presentation. Tabs are draggable; dropping on the center
 * tabs the Content here, dropping near an edge splits beside this
 * Container. Memoized on the Container's object identity — the kernel's
 * structural sharing keeps unchanged Containers identical, so unrelated
 * Containers never re-render.
 */

import { type DragEvent, memo, useState } from "react";
import { type Container, type ContentId, contentIdFrom } from "../index";
import type {
	CommitCommands,
	CompositorChromeLabels,
	ContentLabel,
	RenderContent,
} from "./contracts";
import {
	CONTENT_DRAG_TYPE,
	type DropZone,
	dropCommands,
	dropZoneAt,
} from "./gestures";

interface ContainerViewProps {
	readonly container: Container;
	/** Focused Content id when it lives in this Container, else null. */
	readonly focusedContentId: ContentId | null;
	readonly renderContent: RenderContent;
	readonly contentLabel: ContentLabel;
	readonly labels: CompositorChromeLabels;
	readonly commit: CommitCommands;
}

function draggedContentId(event: DragEvent): ContentId | null {
	const id = event.dataTransfer?.getData(CONTENT_DRAG_TYPE);
	return id ? contentIdFrom(id) : null;
}

function carriesContent(event: DragEvent): boolean {
	const types = event.dataTransfer?.types;
	return Array.isArray(types)
		? types.includes(CONTENT_DRAG_TYPE)
		: Boolean(types && Array.from(types).includes(CONTENT_DRAG_TYPE));
}

export const ContainerView = memo(function ContainerView({
	container,
	focusedContentId,
	renderContent,
	contentLabel,
	labels,
	commit,
}: ContainerViewProps) {
	const [dropZone, setDropZone] = useState<DropZone | null>(null);
	const active =
		container.stack.find(
			(content) => content.id === container.activeContentId,
		) ?? null;
	const zoneOf = (event: DragEvent<HTMLElement>) =>
		dropZoneAt(
			event.currentTarget.getBoundingClientRect(),
			event.clientX,
			event.clientY,
		);
	return (
		<section
			className="oqto-compositor-container"
			data-container-id={container.id}
			data-role={container.role}
			data-drop={dropZone ?? undefined}
			hidden={container.collapsed}
			onDragOver={(event) => {
				if (!carriesContent(event)) return;
				event.preventDefault();
				const zone = zoneOf(event);
				if (zone !== dropZone) setDropZone(zone);
			}}
			onDragLeave={() => setDropZone(null)}
			onDrop={(event) => {
				const contentId = draggedContentId(event);
				const zone = zoneOf(event);
				setDropZone(null);
				if (!contentId) return;
				event.preventDefault();
				commit(dropCommands(contentId, container.id, zone));
			}}
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
								draggable
								onDragStart={(event) => {
									event.dataTransfer.setData(CONTENT_DRAG_TYPE, content.id);
									event.dataTransfer.effectAllowed = "move";
								}}
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
