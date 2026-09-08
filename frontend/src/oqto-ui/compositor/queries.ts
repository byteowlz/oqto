/**
 * Third explicit public seam of the compositor: derived layout queries that
 * answer "where is this true?" without changing anything. Kept separate from
 * index.ts so the primary seam stays within its operation budget; index.ts,
 * navigation.ts and this file are the only legal entries to the kernel.
 */

export { isFrameChrome, resizeBoundaries } from "./kernel/boundaries";
export type {
	BoundaryAxis,
	BoundarySegment,
} from "./kernel/boundaries";
