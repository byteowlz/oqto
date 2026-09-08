/**
 * Adapter-facing render contracts. The compositor never renders Content
 * itself: hosts inject renderers keyed by Content kind, and placement never
 * grants those renderers any authority.
 */

import type { ReactNode } from "react";
import type { ContainerId, ContentRef, LayoutCommand } from "../index";
import type { AddPlacement } from "./gestures";

export interface ContentRenderContext {
	readonly containerId: ContainerId;
	readonly active: boolean;
	readonly focused: boolean;
}

export type RenderContent = (
	content: ContentRef,
	context: ContentRenderContext,
) => ReactNode;

export type ContentLabel = (content: ContentRef) => string;

/** What a Container needs to show Content and to offer more beside it. */
export interface ContentServices {
	readonly render: RenderContent;
	readonly label: ContentLabel;
	/** Content the host offers from a Container's add control. */
	readonly addable: readonly ContentRef[];
	/** Places `content` in the Container; already-placed Content is duplicated. */
	readonly add: (
		content: ContentRef,
		containerId: ContainerId,
		placement: AddPlacement,
	) => void;
}

/** Strings for the add control; `placements` is keyed by where it puts Content. */
export interface AddLabels {
	readonly open: string;
	readonly placements: { readonly [placement in AddPlacement]: string };
}

/** Command palette strings; `actions` maps action ids to labels. */
export interface PaletteLabels {
	readonly title: string;
	readonly searchPlaceholder: string;
	readonly noMatches: string;
	readonly actions: { readonly [id: string]: string };
}

/** Translated chrome strings, injected by the composing host. */
export interface CompositorChromeLabels {
	readonly closeTab: string;
	readonly closeContainer: string;
	readonly resizeColumns: string;
	readonly resizeRows: string;
	readonly dropTop: string;
	readonly dropBottom: string;
	readonly dropStart: string;
	readonly dropEnd: string;
	readonly expandStart: string;
	readonly add: AddLabels;
	readonly palette: PaletteLabels;
}

export type CommitCommands = (commands: readonly LayoutCommand[]) => void;
