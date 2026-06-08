import type { SelectionRect, Viewport } from "../types";

/** Map a canvas-space selection rect to integer source-image pixel bounds. */
export function selectionToSourceRect(
	selection: SelectionRect,
	viewport: Viewport,
): SelectionRect {
	const { scale, offsetX, offsetY, imageWidth, imageHeight } = viewport;
	const sx = (selection.x - offsetX) / scale;
	const sy = (selection.y - offsetY) / scale;
	const sw = selection.width / scale;
	const sh = selection.height / scale;

	const x = Math.max(0, Math.min(imageWidth, sx));
	const y = Math.max(0, Math.min(imageHeight, sy));
	const width = Math.max(1, Math.min(imageWidth - x, sw));
	const height = Math.max(1, Math.min(imageHeight - y, sh));

	return {
		x: Math.round(x),
		y: Math.round(y),
		width: Math.round(width),
		height: Math.round(height),
	};
}

/**
 * Produce a cropped PNG Blob from a source image and a source-space rect using
 * an offscreen 2D canvas. Crop happens on the original pixels (lossless region
 * extraction), independent of the GPU filter preview.
 */
export function cropImageToBlob(
	source: CanvasImageSource,
	rect: SelectionRect,
): Promise<Blob | null> {
	const canvas = document.createElement("canvas");
	canvas.width = rect.width;
	canvas.height = rect.height;
	const ctx = canvas.getContext("2d");
	if (!ctx) return Promise.resolve(null);
	ctx.drawImage(
		source,
		rect.x,
		rect.y,
		rect.width,
		rect.height,
		0,
		0,
		rect.width,
		rect.height,
	);
	return new Promise((resolve) => {
		canvas.toBlob((blob) => resolve(blob), "image/png");
	});
}
