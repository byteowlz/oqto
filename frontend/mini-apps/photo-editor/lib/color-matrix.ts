import type { ColorMatrixFilter } from "pixi.js";
import type { Adjustments } from "../types";

/**
 * Rebuild a ColorMatrixFilter from scratch for the given adjustments. Each
 * adjustment is applied with multiply=true onto a reset (identity) matrix so the
 * combination is stable and re-applying is idempotent.
 */
export function applyAdjustments(
	filter: ColorMatrixFilter,
	adjustments: Adjustments,
): void {
	filter.reset();
	filter.brightness(adjustments.brightness, true);
	filter.contrast(adjustments.contrast, true);
	filter.saturate(adjustments.saturation, true);
}
