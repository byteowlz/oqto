export type Tool = "move" | "crop";

export interface Adjustments {
	/** Brightness multiplier: 1 = unchanged, <1 darker, >1 brighter. */
	brightness: number;
	/** Contrast offset: 0 = unchanged. */
	contrast: number;
	/** Saturation offset: 0 = unchanged, -1 greyscale, >0 more saturated. */
	saturation: number;
}

export const DEFAULT_ADJUSTMENTS: Adjustments = {
	brightness: 1,
	contrast: 0,
	saturation: 0,
};

export interface ImageInfo {
	name: string;
	width: number;
	height: number;
}

export interface ImageLayer {
	visible: boolean;
	opacity: number;
}

export const DEFAULT_LAYER: ImageLayer = {
	visible: true,
	opacity: 1,
};

/** A selection rectangle in canvas (CSS px) coordinates. */
export interface SelectionRect {
	x: number;
	y: number;
	width: number;
	height: number;
}

/** The displayed-image transform, used to map selections back to source px. */
export interface Viewport {
	scale: number;
	offsetX: number;
	offsetY: number;
	imageWidth: number;
	imageHeight: number;
}
