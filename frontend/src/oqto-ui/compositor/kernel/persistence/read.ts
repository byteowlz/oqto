/**
 * Schema v2 document validation: field-by-field narrowing into a
 * LayoutSnapshot, preserving unknown Container/Content/Arrangement
 * variants as extension fields and failing closed on malformed values.
 */

import type {
	ArrangementId,
	ContainerId,
	ContentId,
	GridTrackId,
} from "../ids";
import type {
	ActiveWorkspace,
	Arrangement,
	ArrangementBinding,
	Container,
	ContentRef,
	EmptyBehavior,
	ExtensionFields,
	GridPlacement,
	GridTopology,
	GridTrack,
	JsonValue,
	LayoutSnapshot,
	TrackSize,
} from "../model";
import { LAYOUT_SCHEMA_VERSION } from "../model";
import { extensionsOf, isJsonObject } from "./json-guards";

function readTrackSize(value: JsonValue | undefined): TrackSize | null {
	if (!isJsonObject(value)) return null;
	const { unit, value: length, min } = value;
	if (unit !== "fraction" && unit !== "fixed") return null;
	if (typeof length !== "number" || !Number.isFinite(length) || length <= 0)
		return null;
	if (
		min !== undefined &&
		(typeof min !== "number" || !Number.isFinite(min) || min < 0)
	) {
		return null;
	}
	return min === undefined
		? { unit, value: length }
		: { unit, value: length, min };
}

function readTrack(value: JsonValue): GridTrack | null {
	if (!isJsonObject(value)) return null;
	const size = readTrackSize(value.size);
	if (typeof value.id !== "string" || !size) return null;
	if (value.flush !== undefined && typeof value.flush !== "boolean")
		return null;
	return {
		id: value.id as GridTrackId,
		size,
		...(value.flush === true ? { flush: true } : {}),
	};
}

function readPlacement(value: JsonValue): GridPlacement | null {
	if (!isJsonObject(value)) return null;
	const { containerId, row, column, rowSpan, colSpan } = value;
	if (typeof containerId !== "string") return null;
	const numbers = [row, column, rowSpan, colSpan];
	if (!numbers.every((n) => typeof n === "number" && Number.isInteger(n)))
		return null;
	return {
		containerId: containerId as ContainerId,
		row: row as number,
		column: column as number,
		rowSpan: rowSpan as number,
		colSpan: colSpan as number,
	};
}

function readGrid(value: JsonValue | undefined): GridTopology | null {
	if (!isJsonObject(value)) return null;
	const { rows, columns, placements } = value;
	if (
		!Array.isArray(rows) ||
		!Array.isArray(columns) ||
		!Array.isArray(placements)
	)
		return null;
	const readRows = rows.map(readTrack);
	const readColumns = columns.map(readTrack);
	const readPlacements = placements.map(readPlacement);
	if (
		[...readRows, ...readColumns, ...readPlacements].some(
			(item) => item === null,
		)
	)
		return null;
	return {
		rows: readRows as GridTrack[],
		columns: readColumns as GridTrack[],
		placements: readPlacements as GridPlacement[],
	};
}

function readBinding(
	value: JsonValue | undefined,
): ArrangementBinding | null | undefined {
	if (value === undefined) return undefined;
	if (
		!isJsonObject(value) ||
		typeof value.kind !== "string" ||
		typeof value.id !== "string"
	)
		return null;
	return { kind: value.kind, id: value.id };
}

const ARRANGEMENT_KEYS = [
	"id",
	"label",
	"binding",
	"grid",
	"scrollAnchorColumnId",
	"lastFocusedContentId",
];

function readArrangement(value: JsonValue): Arrangement | null {
	if (!isJsonObject(value)) return null;
	const { id, label, scrollAnchorColumnId, lastFocusedContentId } = value;
	const grid = readGrid(value.grid);
	const binding = readBinding(value.binding);
	if (typeof id !== "string" || !grid || binding === null) return null;
	if (label !== undefined && typeof label !== "string") return null;
	if (scrollAnchorColumnId !== null && typeof scrollAnchorColumnId !== "string")
		return null;
	if (lastFocusedContentId !== null && typeof lastFocusedContentId !== "string")
		return null;
	const extensions = extensionsOf(value, ARRANGEMENT_KEYS);
	return {
		id: id as ArrangementId,
		...(label !== undefined ? { label } : {}),
		...(binding !== undefined ? { binding } : {}),
		grid,
		scrollAnchorColumnId: scrollAnchorColumnId as GridTrackId | null,
		lastFocusedContentId: lastFocusedContentId as ContentId | null,
		...(extensions ? { extensions } : {}),
	};
}

function readContent(value: JsonValue): ContentRef | null {
	if (!isJsonObject(value)) return null;
	if (typeof value.id !== "string" || typeof value.kind !== "string")
		return null;
	const extensions = extensionsOf(value, ["id", "kind"]);
	return {
		id: value.id as ContentId,
		kind: value.kind,
		...(extensions ? { extensions } : {}),
	};
}

const CONTAINER_KEYS = [
	"id",
	"role",
	"emptyBehavior",
	"collapsed",
	"activeContentId",
	"stack",
];

function readContainer(value: JsonValue): Container | null {
	if (!isJsonObject(value)) return null;
	const { id, role, emptyBehavior, collapsed, activeContentId, stack } = value;
	if (typeof id !== "string" || typeof collapsed !== "boolean") return null;
	if (
		emptyBehavior !== "retain" &&
		emptyBehavior !== "collapse" &&
		emptyBehavior !== "remove"
	)
		return null;
	if (activeContentId !== null && typeof activeContentId !== "string")
		return null;
	if (role !== undefined && typeof role !== "string") return null;
	if (!Array.isArray(stack)) return null;
	const contents = stack.map(readContent);
	if (contents.some((content) => content === null)) return null;
	const extensions = extensionsOf(value, CONTAINER_KEYS);
	return {
		id: id as ContainerId,
		...(role !== undefined ? { role } : {}),
		emptyBehavior: emptyBehavior as EmptyBehavior,
		stack: contents as ContentRef[],
		activeContentId: activeContentId as ContentId | null,
		collapsed,
		...(extensions ? { extensions } : {}),
	};
}

function readWorkspace(value: JsonValue | undefined): ActiveWorkspace | null {
	if (!isJsonObject(value)) return null;
	if (value.kind === "all") return { kind: "all" };
	if (
		value.kind === "workspace" &&
		typeof value.id === "string" &&
		value.id.length > 0
	) {
		return { kind: "workspace", id: value.id };
	}
	return null;
}

const SNAPSHOT_KEYS = [
	"schemaVersion",
	"revision",
	"idSeed",
	"strategy",
	"activeArrangementId",
	"activeWorkspace",
	"arrangements",
	"containers",
	"focusedContentId",
];

export function readSnapshot(
	layoutDocument: ExtensionFields,
): LayoutSnapshot | null {
	const {
		schemaVersion,
		revision,
		idSeed,
		strategy,
		activeArrangementId,
		arrangements,
		containers,
		focusedContentId,
	} = layoutDocument;
	const activeWorkspace = readWorkspace(layoutDocument.activeWorkspace);
	if (
		schemaVersion !== LAYOUT_SCHEMA_VERSION ||
		typeof revision !== "number" ||
		!Number.isInteger(revision) ||
		revision < 0 ||
		typeof idSeed !== "number" ||
		!Number.isInteger(idSeed) ||
		idSeed < 0 ||
		strategy !== "grid" ||
		typeof activeArrangementId !== "string" ||
		!activeWorkspace ||
		!Array.isArray(arrangements) ||
		!Array.isArray(containers) ||
		(focusedContentId !== null && typeof focusedContentId !== "string")
	) {
		return null;
	}
	const readArrangements = arrangements.map(readArrangement);
	const readContainers = containers.map(readContainer);
	if (
		readArrangements.some((item) => item === null) ||
		readContainers.some((item) => item === null)
	) {
		return null;
	}
	const extensions = extensionsOf(layoutDocument, SNAPSHOT_KEYS);
	return {
		schemaVersion: LAYOUT_SCHEMA_VERSION,
		revision,
		idSeed,
		strategy: "grid",
		activeArrangementId: activeArrangementId as ArrangementId,
		activeWorkspace,
		arrangements: readArrangements as Arrangement[],
		containers: readContainers as Container[],
		focusedContentId: focusedContentId as ContentId | null,
		...(extensions ? { extensions } : {}),
	};
}
