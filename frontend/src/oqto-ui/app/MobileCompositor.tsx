/**
 * Mobile Screen Mode projection (ADR-0041/0043): the same placed Content
 * projected into one focused destination (Chat or Files/Settings by
 * `mobileView`), with navigation as a full-screen drawer and the OG mobile
 * chrome classes so the shell stylesheet applies. It never mutates the
 * layout document; it is a read-only projection over the desktop
 * Arrangement.
 */

import { useSyncExternalStore } from "react";
import type { ContentRef, LayoutSnapshot } from "../compositor/index";
import type {
	ContentRenderContext,
	RenderContent,
} from "../compositor/react/contracts";
import type { CompositorStore } from "../compositor/react/store";

interface MobileCompositorProps {
	readonly store: CompositorStore;
	readonly mobileView: string;
	readonly renderContent: RenderContent;
}

/** Placed Content of the active Arrangement in reading order. */
function placedContent(snapshot: LayoutSnapshot): ContentRef[] {
	const arrangement = snapshot.arrangements.find(
		(candidate) => candidate.id === snapshot.activeArrangementId,
	);
	if (!arrangement) return [];
	const order = [...arrangement.grid.placements].sort(
		(a, b) => a.row - b.row || a.column - b.column,
	);
	return order.flatMap(
		(placement) =>
			snapshot.containers.find((c) => c.id === placement.containerId)?.stack ??
			[],
	);
}

export function MobileCompositor({
	store,
	mobileView,
	renderContent,
}: MobileCompositorProps) {
	const snapshot = useSyncExternalStore(
		store.subscribe,
		store.getSnapshot,
		store.getSnapshot,
	);
	const placed = placedContent(snapshot);
	const focused = placed.find(
		(content) => content.id === snapshot.focusedContentId,
	);
	const chat =
		focused?.kind === "chat"
			? focused
			: placed.find((content) => content.kind === "chat");
	const aside =
		placed.find((content) => content.kind === "settings") ??
		placed.find((content) => content.kind === "files");
	const sessions = placed.find((content) => content.kind === "sessions");
	const context = (
		content: ContentRef,
		active: boolean,
	): ContentRenderContext => {
		const owner =
			snapshot.containers.find((c) =>
				c.stack.some((item) => item.id === content.id),
			) ?? snapshot.containers[0];
		return {
			containerId: owner.id,
			active,
			focused: snapshot.focusedContentId === content.id,
		};
	};
	return (
		<>
			{sessions ? renderContent(sessions, context(sessions, true)) : null}
			<div
				className="wb-workarea"
				data-view={mobileView}
				data-compositor-mobile="true"
			>
				{chat
					? renderContent(chat, context(chat, mobileView !== "files"))
					: null}
				{aside
					? renderContent(aside, context(aside, mobileView === "files"))
					: null}
			</div>
		</>
	);
}
