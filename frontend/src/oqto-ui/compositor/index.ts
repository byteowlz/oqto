/**
 * Public seam of the OqtoUI Container compositor (ADR-0041/0042/0043).
 * Everything outside the kernel — React adapters, layout chrome,
 * Customizations, and future Agent-control brokers — consumes exactly this
 * interface. Kernel internals are guardrail-enforced private.
 */

export { applyTransaction } from "./kernel/engine";
export { solveLayoutGeometry } from "./kernel/geometry";
export { contentIdFrom } from "./kernel/ids";
export { encodeLayoutDocument, recoverLayoutDocument } from "./kernel/persistence/codec";
export { createClassicPresetLayout } from "./kernel/preset";
export { projectLayout } from "./kernel/projection";

export type {
	ApplyResult,
	LayoutCommand,
	LayoutEvent,
	LayoutRejection,
	LayoutTransaction,
	MoveDestination,
	OpenTarget,
	ResizeAxis,
	SplitEdge,
} from "./kernel/commands";
export type {
	GeometryDegradation,
	SafeAreaInsets,
	SolvedLayout,
	SolvedRect,
	ViewportConstraints,
} from "./kernel/geometry";
export type { ArrangementId, ContainerId, ContentId, GridTrackId, LayoutRevision } from "./kernel/ids";
export type { InvariantViolation } from "./kernel/invariants";
export type {
	ActiveWorkspace,
	Arrangement,
	ArrangementBinding,
	Container,
	ContentKind,
	ContentRef,
	EmptyBehavior,
	ExtensionFields,
	GridPlacement,
	GridTopology,
	GridTrack,
	JsonValue,
	LayoutSnapshot,
	TrackSize,
} from "./kernel/model";
export type { DecodeFailureReason, DecodeLayoutResult, RecoveredLayout } from "./kernel/persistence/codec";
export type { LayoutMigration } from "./kernel/persistence/migrations";
export type { ClassicPresetContent } from "./kernel/preset";
export type { MergeProvenance, ProjectedLayout, ViewportClass } from "./kernel/projection";
