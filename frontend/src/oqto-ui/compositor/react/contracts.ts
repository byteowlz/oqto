/**
 * Adapter-facing render contracts. The compositor never renders Content
 * itself: hosts inject renderers keyed by Content kind, and placement never
 * grants those renderers any authority.
 */

import type { ReactNode } from "react";
import type {
	ContainerId,
	ContentRef,
	LayoutCommand,
	SplitEdge,
} from "../index";

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
	/** Content the host offers when adding a Container beside this one. */
	readonly addable: readonly ContentRef[];
}

/** Strings for the add affordance; `edges` is keyed by the direction. */
export interface AddLabels {
	readonly edges: { readonly [edge in SplitEdge]: string };
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
