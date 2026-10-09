/**
 * Semantic command vocabulary and typed transaction results (ADR-0041,
 * extended by ADR-0042/0043). All human input, Customizations, restored
 * state, and future Agent control use these commands; nothing mutates
 * Container arrays directly. `preview`/`apply` are host workflow over the
 * pure engine: previewing is calling `applyTransaction` without committing.
 */

import type { ViewportConstraints } from "./geometry";
import type {
	ArrangementId,
	ContainerId,
	ContentId,
	GridTrackId,
	LayoutRevision,
} from "./ids";
import type {
	ActiveWorkspace,
	ArrangementBinding,
	ContentRef,
	LayoutSnapshot,
	TrackSize,
} from "./model";

export interface OpenTarget {
	readonly containerId?: ContainerId;
	readonly arrangementId?: ArrangementId;
	readonly role?: string;
	readonly position?: number;
}

export interface MoveDestination {
	readonly containerId?: ContainerId;
	/** Without a Container: the Arrangement's default (role/primary) target. */
	readonly arrangementId?: ArrangementId;
	readonly position?: number;
}

export type SplitEdge =
	| "inline-start"
	| "inline-end"
	| "block-start"
	| "block-end";

export type ResizeAxis = "inline" | "block";

export type LayoutCommand =
	| {
			readonly type: "open";
			readonly content: ContentRef;
			readonly target?: OpenTarget;
	  }
	| { readonly type: "reveal"; readonly contentId: ContentId }
	| { readonly type: "activate"; readonly contentId: ContentId }
	| { readonly type: "focus"; readonly contentId: ContentId }
	| {
			readonly type: "move";
			readonly contentId: ContentId;
			readonly destination: MoveDestination;
	  }
	| {
			/** Without `relativeTo`, splits at the active Arrangement's edge. */
			readonly type: "split";
			readonly contentId: ContentId;
			readonly edge: SplitEdge;
			readonly relativeTo?: ContainerId;
	  }
	| {
			readonly type: "resize";
			readonly containerId: ContainerId;
			readonly axis: ResizeAxis;
			readonly size: TrackSize;
	  }
	| {
			readonly type: "collapse";
			readonly containerId: ContainerId;
			readonly collapsed: boolean;
	  }
	| {
			readonly type: "flush";
			readonly rowId: GridTrackId;
			readonly flush: boolean;
	  }
	| {
			readonly type: "scroll";
			readonly anchorColumnId: GridTrackId | null;
	  }
	| { readonly type: "close"; readonly contentId: ContentId }
	| {
			readonly type: "arrangement-create";
			readonly label?: string;
			readonly binding?: ArrangementBinding;
			readonly activate?: boolean;
	  }
	| {
			readonly type: "arrangement-switch";
			readonly arrangementId: ArrangementId;
	  }
	| {
			readonly type: "arrangement-remove";
			readonly arrangementId: ArrangementId;
	  }
	| { readonly type: "workspace-switch"; readonly workspace: ActiveWorkspace }
	| { readonly type: "undo"; readonly snapshot: LayoutSnapshot };

export interface LayoutTransaction {
	readonly expectedRevision: LayoutRevision;
	readonly commands: readonly LayoutCommand[];
	/** Lets reveal/open scroll precisely; without it the kernel only scrolls left. */
	readonly viewport?: ViewportConstraints;
}

export type LayoutEvent =
	| {
			readonly type: "opened";
			readonly contentId: ContentId;
			readonly containerId: ContainerId;
	  }
	| {
			readonly type: "revealed";
			readonly contentId: ContentId;
			readonly containerId: ContainerId;
	  }
	| {
			readonly type: "activated";
			readonly contentId: ContentId;
			readonly containerId: ContainerId;
	  }
	| { readonly type: "focused"; readonly contentId: ContentId }
	| {
			readonly type: "moved";
			readonly contentId: ContentId;
			readonly from: ContainerId;
			readonly to: ContainerId;
	  }
	| {
			readonly type: "split";
			readonly containerId: ContainerId;
			readonly edge: SplitEdge;
	  }
	| {
			readonly type: "resized";
			readonly containerId: ContainerId;
			readonly axis: ResizeAxis;
	  }
	| {
			readonly type: "collapsed";
			readonly containerId: ContainerId;
			readonly collapsed: boolean;
	  }
	| {
			readonly type: "flushed";
			readonly rowId: GridTrackId;
			readonly flush: boolean;
	  }
	| {
			readonly type: "scrolled";
			readonly arrangementId: ArrangementId;
			readonly anchorColumnId: GridTrackId | null;
	  }
	| {
			readonly type: "closed";
			readonly contentId: ContentId;
			readonly containerId: ContainerId;
	  }
	| { readonly type: "container-removed"; readonly containerId: ContainerId }
	| {
			readonly type: "arrangement-created";
			readonly arrangementId: ArrangementId;
	  }
	| {
			readonly type: "arrangement-switched";
			readonly from: ArrangementId;
			readonly to: ArrangementId;
	  }
	| {
			readonly type: "arrangement-removed";
			readonly arrangementId: ArrangementId;
	  }
	| { readonly type: "workspace-switched"; readonly workspace: ActiveWorkspace }
	| { readonly type: "undone"; readonly restoredRevision: LayoutRevision };

export type LayoutRejection =
	| {
			readonly reason: "revision-conflict";
			readonly expected: LayoutRevision;
			readonly actual: LayoutRevision;
	  }
	| { readonly reason: "not-placed"; readonly contentId: ContentId }
	| { readonly reason: "unknown-content"; readonly contentId: ContentId }
	| { readonly reason: "unknown-container"; readonly containerId: ContainerId }
	| {
			readonly reason: "unknown-arrangement";
			readonly arrangementId: ArrangementId;
	  }
	| {
			readonly reason: "last-arrangement";
			readonly arrangementId: ArrangementId;
	  }
	| { readonly reason: "invalid-command"; readonly detail: string }
	| { readonly reason: "invariant-violation"; readonly detail: string };

export type ApplyResult =
	| {
			readonly ok: true;
			readonly snapshot: LayoutSnapshot;
			readonly events: readonly LayoutEvent[];
	  }
	| { readonly ok: false; readonly rejection: LayoutRejection };

/** Internal per-command outcome; the engine composes these atomically. */
export type CommandOutcome =
	| {
			readonly ok: true;
			readonly state: LayoutSnapshot;
			readonly events: readonly LayoutEvent[];
	  }
	| { readonly ok: false; readonly rejection: LayoutRejection };
