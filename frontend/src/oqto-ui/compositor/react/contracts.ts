/**
 * Adapter-facing render contracts. The compositor never renders Content
 * itself: hosts inject renderers keyed by Content kind, and placement never
 * grants those renderers any authority.
 */

import type { ReactNode } from "react";
import type { ContainerId, ContentRef, LayoutCommand } from "../index";

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

/** Translated chrome strings, injected by the composing host. */
export interface CompositorChromeLabels {
	readonly closeTab: string;
	readonly resizeColumns: string;
	readonly resizeRows: string;
	readonly dropTop: string;
	readonly dropBottom: string;
	readonly dropStart: string;
	readonly dropEnd: string;
}

export type CommitCommands = (commands: readonly LayoutCommand[]) => void;
