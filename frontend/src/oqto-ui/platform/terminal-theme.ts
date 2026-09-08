/**
 * The terminal's colours, read from the theme's own custom properties.
 * Browser globals live in platform adapters, so the pane receives values
 * rather than reaching into the document itself.
 */

export interface TerminalTheme {
	readonly background: string;
	readonly foreground: string;
	readonly fontFamily: string;
}

/**
 * Only reached when the document has no theme at all (a detached render).
 * The emulator needs literal colours, so these name the CSS system colours
 * rather than inventing a palette.
 */
const FALLBACK: TerminalTheme = {
	background: "canvas",
	foreground: "canvastext",
	fontFamily: "ui-monospace, monospace",
};

export function readTerminalTheme(): TerminalTheme {
	try {
		const style = getComputedStyle(document.documentElement);
		const read = (name: string, fallback: string) =>
			style.getPropertyValue(name).trim() || fallback;
		return {
			background: read("--terminal-bg", FALLBACK.background),
			foreground: read("--terminal-fg", FALLBACK.foreground),
			fontFamily: read("--font-mono", FALLBACK.fontFamily),
		};
	} catch {
		return FALLBACK;
	}
}
