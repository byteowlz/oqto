/**
 * Canonical Layout model, schema v2 (ADR-0041/0042/0043). Pure data: stable
 * references, tracks, placements, constraints, ordering, and
 * schema/revision metadata. Never messages, file contents, App data,
 * secrets, sockets, grants, observed rectangles, or DOM measurements —
 * solved geometry is derived, not stored.
 *
 * Vocabulary: Screen Mode -> Arrangement -> Lane (row track) -> Container
 * -> Content tabs. A Screen Mode's document is one LayoutSnapshot.
 */

import type {
	ArrangementId,
	ContainerId,
	ContentId,
	GridTrackId,
	LayoutRevision,
} from "./ids";

export type JsonValue =
	| string
	| number
	| boolean
	| null
	| readonly JsonValue[]
	| { readonly [key: string]: JsonValue };

/** Unrecognized persisted fields, preserved for round-trip survival. */
export type ExtensionFields = { readonly [key: string]: JsonValue };

/** First-party Content kinds; App and future kinds are open strings. */
export type ContentKind = string;

export interface ContentRef {
	readonly id: ContentId;
	readonly kind: ContentKind;
	readonly extensions?: ExtensionFields;
}

export type EmptyBehavior = "retain" | "collapse" | "remove";

/** Semantic logical length in host-local units; never CSS. */
export interface TrackSize {
	readonly unit: "fraction" | "fixed";
	readonly value: number;
	readonly min?: number;
}

export interface Container {
	readonly id: ContainerId;
	readonly role?: string;
	readonly emptyBehavior: EmptyBehavior;
	readonly stack: readonly ContentRef[];
	readonly activeContentId: ContentId | null;
	readonly collapsed: boolean;
	readonly extensions?: ExtensionFields;
}

/** A row or column track. `flush` is meaningful only on a first/last row. */
export interface GridTrack {
	readonly id: GridTrackId;
	readonly size: TrackSize;
	readonly flush?: boolean;
}

/** Zero-based track indexes with spans, like CSS grid lines minus one. */
export interface GridPlacement {
	readonly containerId: ContainerId;
	readonly row: number;
	readonly column: number;
	readonly rowSpan: number;
	readonly colSpan: number;
}

export interface GridTopology {
	readonly rows: readonly GridTrack[];
	readonly columns: readonly GridTrack[];
	readonly placements: readonly GridPlacement[];
}

/** Organizational reference only; never authority (ADR-0043). */
export interface ArrangementBinding {
	readonly kind: string;
	readonly id: string;
}

export interface Arrangement {
	readonly id: ArrangementId;
	readonly label?: string;
	readonly binding?: ArrangementBinding;
	readonly grid: GridTopology;
	/** Column track the visible window starts at; null means the start. */
	readonly scrollAnchorColumnId: GridTrackId | null;
	readonly lastFocusedContentId: ContentId | null;
	readonly extensions?: ExtensionFields;
}

export type ActiveWorkspace =
	| { readonly kind: "workspace"; readonly id: string }
	| { readonly kind: "all" };

export interface LayoutSnapshot {
	readonly schemaVersion: number;
	readonly revision: LayoutRevision;
	readonly idSeed: number;
	readonly strategy: "grid";
	readonly activeArrangementId: ArrangementId;
	readonly activeWorkspace: ActiveWorkspace;
	readonly arrangements: readonly Arrangement[];
	readonly containers: readonly Container[];
	readonly focusedContentId: ContentId | null;
	readonly extensions?: ExtensionFields;
}

export const LAYOUT_SCHEMA_VERSION = 2;
