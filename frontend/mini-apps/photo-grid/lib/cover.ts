import type { GridState, PoolImage } from "../types";

function clamp(value: number, min: number, max: number): number {
	return Math.max(min, Math.min(max, value));
}

/**
 * Draw an image into a destination rect using object-fit: cover semantics with
 * an object-position framing (posX/posY in 0..100). Mirrors exactly how the
 * tiles render on screen, so export matches the preview.
 */
export function drawCover(
	ctx: CanvasRenderingContext2D,
	img: CanvasImageSource,
	dx: number,
	dy: number,
	dw: number,
	dh: number,
	posX: number,
	posY: number,
): void {
	const iw =
		(img as HTMLImageElement).naturalWidth || (img as HTMLImageElement).width;
	const ih =
		(img as HTMLImageElement).naturalHeight || (img as HTMLImageElement).height;
	if (!iw || !ih) return;
	const scale = Math.max(dw / iw, dh / ih);
	const sw = dw / scale;
	const sh = dh / scale;
	const maxSx = Math.max(0, iw - sw);
	const maxSy = Math.max(0, ih - sh);
	const sx = clamp((posX / 100) * maxSx, 0, maxSx);
	const sy = clamp((posY / 100) * maxSy, 0, maxSy);
	ctx.drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh);
}

/** Cumulative pixel rects for each tile given fractional sizes and gaps. */
export function tileRects(
	state: GridState,
	outW: number,
	outH: number,
	gapPx: number,
): Array<{ x: number; y: number; w: number; h: number }> {
	const { cols, rows } = state.spec;
	const availW = outW - gapPx * (cols - 1);
	const availH = outH - gapPx * (rows - 1);

	const colPx = state.colSizes.map((f) => f * availW);
	const rowPx = state.rowSizes.map((f) => f * availH);

	const colX: number[] = [];
	let cx = 0;
	for (let c = 0; c < cols; c++) {
		colX.push(cx);
		cx += colPx[c] + gapPx;
	}
	const rowY: number[] = [];
	let cy = 0;
	for (let r = 0; r < rows; r++) {
		rowY.push(cy);
		cy += rowPx[r] + gapPx;
	}

	const rects: Array<{ x: number; y: number; w: number; h: number }> = [];
	for (let r = 0; r < rows; r++) {
		for (let c = 0; c < cols; c++) {
			rects.push({ x: colX[c], y: rowY[r], w: colPx[c], h: rowPx[r] });
		}
	}
	return rects;
}

/**
 * Composite the grid into a single PNG Blob at the requested output size.
 * Empty tiles and gaps are filled with `background`.
 */
export function composeGrid(
	state: GridState,
	images: Map<string, HTMLImageElement>,
	outW: number,
	outH: number,
	gapPx: number,
	background: string,
): Promise<Blob | null> {
	const canvas = document.createElement("canvas");
	canvas.width = Math.round(outW);
	canvas.height = Math.round(outH);
	const ctx = canvas.getContext("2d");
	if (!ctx) return Promise.resolve(null);

	ctx.fillStyle = background;
	ctx.fillRect(0, 0, canvas.width, canvas.height);

	const rects = tileRects(state, canvas.width, canvas.height, gapPx);
	state.tiles.forEach((tile, i) => {
		const rect = rects[i];
		if (!rect) return;
		if (!tile.imageId) return;
		const img = images.get(tile.imageId);
		if (!img) return;
		ctx.save();
		ctx.beginPath();
		ctx.rect(rect.x, rect.y, rect.w, rect.h);
		ctx.clip();
		drawCover(ctx, img, rect.x, rect.y, rect.w, rect.h, tile.posX, tile.posY);
		ctx.restore();
	});

	return new Promise((resolve) => {
		canvas.toBlob((blob) => resolve(blob), "image/png");
	});
}

/** Build a quick lookup of pool images by id (used by the compositor). */
export function poolById(pool: PoolImage[]): Map<string, PoolImage> {
	return new Map(pool.map((p) => [p.id, p]));
}
