/**
 * Normalize an emitted token's computed value to #rrggbb, for UI controls
 * (e.g. <input type="color">) that only accept 6-digit hex. Canvas is the only
 * universally available parser that resolves arbitrary CSS color syntax
 * (oklch(), color(), keywords); environments without a 2d canvas (jsdom) fall
 * back to a neutral mid gray.
 */

export const NEUTRAL_TOKEN_HEX = "#888888";

const HEX_COLOR = /^#[0-9a-f]{6}$/i;
const RGB_COLOR = /^rgba?\((\d+),\s*(\d+),\s*(\d+)/;

export function readTokenHex(root: HTMLElement, token: string): string {
	const raw = getComputedStyle(root).getPropertyValue(token).trim();
	if (HEX_COLOR.test(raw)) return raw.toLowerCase();
	const context = document.createElement("canvas").getContext("2d");
	if (!context || raw === "") return NEUTRAL_TOKEN_HEX;
	context.fillStyle = NEUTRAL_TOKEN_HEX;
	context.fillStyle = raw;
	const parsed = String(context.fillStyle);
	if (HEX_COLOR.test(parsed)) return parsed.toLowerCase();
	const rgb = RGB_COLOR.exec(parsed);
	if (!rgb || rgb.length < 4) return NEUTRAL_TOKEN_HEX;
	const hex = (value: string | undefined) =>
		Number.parseInt(value ?? "136", 10)
			.toString(16)
			.padStart(2, "0");
	return `#${hex(rgb[1])}${hex(rgb[2])}${hex(rgb[3])}`;
}
