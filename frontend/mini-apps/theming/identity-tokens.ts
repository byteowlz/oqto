/**
 * Identity tokens: the non-color parts of oqto's visual identity that stay
 * constant across every base24 scheme (radius, fonts, shadows, spacing,
 * tracking). The theming engine never overwrites these from a scheme; they are
 * applied once so a standalone workbench renders with oqto's true geometry and
 * typography even though globals.css is not the active source of truth there.
 *
 * Values mirror frontend/src/styles/globals.css. Keep in sync until the
 * integration phase generates globals.css from the scheme + identity tokens.
 */
export const IDENTITY_TOKENS: Readonly<Record<string, string>> = {
	"--radius": "0",
	"--radius-sm": "0",
	"--radius-md": "0",
	"--radius-lg": "0",
	"--radius-xl": "0",
	"--font-sans": '"JetBrainsMono Nerd Font", ui-monospace, monospace',
	"--font-serif": '"JetBrainsMono Nerd Font", ui-monospace, monospace',
	"--font-mono": '"JetBrainsMono Nerd Font", ui-monospace, monospace',
	"--spacing": "0.25rem",
	"--tracking-normal": "0em",
	"--letter-spacing": "0em",
};

/** Apply the constant identity tokens to a root element. */
export function applyIdentityTokens(root: HTMLElement): void {
	for (const [name, value] of Object.entries(IDENTITY_TOKENS)) {
		root.style.setProperty(name, value);
	}
}
