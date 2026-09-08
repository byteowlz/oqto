/**
 * One Container: its Content, and a tab bar only once it holds more than
 * one. With a single Content the Container's controls travel to the
 * presentation instead, so no header band appears above a pane that already
 * has one. Tabs are draggable; dropping on the center tabs the Content
 * here, dropping near an edge splits beside this Container. Memoized on the
 * Container's object identity — the kernel's structural sharing keeps
 * unchanged Containers identical, so unrelated Containers never re-render.
 */

import { type DragEvent, memo, useState } from "react";
import { type Container, type ContentId, contentIdFrom } from "../index";
import { isFrameChrome } from "../queries";
import { ContainerChrome } from "./ContainerChrome";
import type {
	CommitCommands,
	CompositorChromeLabels,
	ContentServices,
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
	readonly content: ContentServices;
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
	content: services,
	labels,
	commit,
}: ContainerViewProps) {
	const [dropZone, setDropZone] = useState<DropZone | null>(null);
	const active =
		container.stack.find(
			(content) => content.id === container.activeContentId,
		) ?? null;
	// A Container with several Contents needs a tab bar, and the controls
	// belong in it; a Container with one hands them to the presentation.
	const tabbed = container.stack.length > 1;
	const chrome = isFrameChrome(container) ? null : (
		<ContainerChrome
			containerId={container.id}
			stack={container.stack}
			content={services}
			labels={labels}
			commit={commit}
		/>
	);
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
			{tabbed ? (
				<div className="oqto-compositor-tabs" role="tablist">
					{container.stack.map((item) => (
						<div
							key={item.id}
							className="oqto-compositor-tab"
							data-active={item.id === container.activeContentId || undefined}
						>
							<button
								type="button"
								role="tab"
								aria-selected={item.id === container.activeContentId}
								draggable
								onDragStart={(event) => {
									event.dataTransfer.setData(CONTENT_DRAG_TYPE, item.id);
									event.dataTransfer.effectAllowed = "move";
								}}
								onClick={() =>
									commit([{ type: "activate", contentId: item.id }])
								}
							>
								{services.label(item)}
							</button>
							<button
								type="button"
								className="oqto-compositor-tab-close"
								aria-label={labels.closeTab}
								onClick={() => commit([{ type: "close", contentId: item.id }])}
							>
								{"×"}
							</button>
						</div>
					))}
					{chrome}
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
					services.render(active, {
						containerId: container.id,
						active: true,
						focused: focusedContentId === active.id,
						chrome: tabbed ? null : chrome,
					})
				) : (
					<div className="oqto-compositor-empty" />
				)}
			</div>
		</section>
	);
});
