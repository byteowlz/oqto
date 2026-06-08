export interface GridSpec {
	rows: number;
	cols: number;
}

export interface PoolImage {
	id: string;
	name: string;
	/** Object URL for display. */
	url: string;
	width: number;
	height: number;
}

export interface Tile {
	imageId: string | null;
	/** object-position framing of the cover crop, 0..100 (%). */
	posX: number;
	posY: number;
}

/** Outer insets of the grid within its frame, as fractions of the frame. */
export interface Margins {
	top: number;
	right: number;
	bottom: number;
	left: number;
}

export const ZERO_MARGINS: Margins = { top: 0, right: 0, bottom: 0, left: 0 };

export interface GridState {
	spec: GridSpec;
	/** Column width fractions, length = cols, summing to ~1. */
	colSizes: number[];
	/** Row height fractions, length = rows, summing to ~1. */
	rowSizes: number[];
	/** Tiles in row-major order, length = rows * cols. */
	tiles: Tile[];
	/** Uploaded image pool (can exceed tile count). */
	pool: PoolImage[];
	/** Gap between tiles, in px. */
	gap: number;
	/** Per-side outer insets (drag the outer edges to resize the whole grid). */
	margins: Margins;
}

export const MIN_FRACTION = 0.06;
/** The grid must always keep at least this fraction of the frame per axis. */
export const MIN_GRID_SPAN = 0.2;
export const MAX_ROWS = 6;
export const MAX_COLS = 6;

export function evenSizes(n: number): number[] {
	return Array.from({ length: n }, () => 1 / n);
}

export function emptyTile(): Tile {
	return { imageId: null, posX: 50, posY: 50 };
}

export function makeTiles(count: number): Tile[] {
	return Array.from({ length: count }, () => emptyTile());
}
