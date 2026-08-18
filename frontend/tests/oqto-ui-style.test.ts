import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const shellCss = readFileSync(
	resolve(process.cwd(), "src/oqto-ui/app/shell.css"),
	"utf8",
);
const lightSchemes = ["oqto-light.json", "nord-light.json"].map((fileName) =>
	JSON.parse(
		readFileSync(
			resolve(process.cwd(), "vendor/design-system/src/schemes", fileName),
			"utf8",
		),
	),
);

describe("OqtoUI visual stability", () => {
	it("has no ambient animation or transition declarations", () => {
		expect(shellCss).not.toMatch(/@keyframes|\banimation\s*:|\btransition\s*:/);
	});

	it("keeps the button reset lower-specificity than component spacing", () => {
		expect(shellCss).toContain(".wb-shell :where(button)");
		expect(shellCss).not.toMatch(/\.wb-shell button\s*\{/);
	});

	it("collapses mobile chrome to one bar and hides duplicated headers", () => {
		const mobileBlock = shellCss.slice(
			shellCss.indexOf("@media (max-width: 1023px)"),
		);
		expect(mobileBlock).toMatch(/\.wb-wa-tabs\s*\{\s*display:\s*none;/);
		// The chat header row was removed entirely; the chat tab carries the
		// gauge and hover meta instead, so no header selector may come back.
		expect(shellCss).not.toMatch(/\.wb-chat-header/);
		expect(mobileBlock).toMatch(/\.wb-mobile-chrome\s*\{[^}]*display:\s*grid;/);
		expect(mobileBlock).toMatch(/\.wb-sidebar\s*\{[^}]*position:\s*fixed;/);
	});

	it("counters the app-wide flatten reset inside the lab shell", () => {
		expect(shellCss).toMatch(/border-radius: 50% !important/);
		expect(shellCss).toMatch(/border-radius: var\(--radius-lg, 0\) !important/);
		expect(shellCss).toMatch(/border-radius: var\(--radius-sm, 0\) !important/);
	});

	it("consumes the radius and effect tokens instead of hardcoding depth", () => {
		expect(shellCss).toMatch(/border-radius: var\(--radius-lg, 0\)/);
		expect(shellCss).toMatch(/border-radius: var\(--radius-sm, 0\)/);
		expect(shellCss).toMatch(/box-shadow: var\(--shadow-lg, none\)/);
		expect(shellCss).toMatch(
			/backdrop-filter: blur\(var\(--backdrop-blur, 0px\)\)/,
		);
		expect(shellCss).not.toMatch(/border-radius:\s*\d+px/);
		expect(shellCss).not.toMatch(/box-shadow:\s*0 /);
	});

	it("keeps light-theme semantic overrides within their Base24 palettes", () => {
		for (const scheme of lightSchemes) {
			for (const value of Object.values(scheme.overrides)) {
				expect(value).toMatch(/^var\(--base[0-1][0-9A-F]\)$/);
			}
		}
	});

	it("maps the same light-theme roles to the same Base24 slots", () => {
		const [oqtoLight, nordLight] = lightSchemes;
		expect(nordLight.overrides).toEqual(oqtoLight.overrides);
		expect(nordLight.overrides["--background"]).toBe("var(--base02)");
		expect(nordLight.overrides["--card"]).toBe("var(--base01)");
		expect(nordLight.overrides["--sidebar"]).toBe("var(--base02)");
		expect(nordLight.overrides["--border"]).toBe("var(--base03)");
		expect(nordLight.overrides["--input"]).toBe("var(--base02)");
		expect(nordLight.overrides["--code-accent"]).toBe("var(--base0B)");
	});

	it("keeps the canonical Tinted Theming Nord Light Base16 slots unchanged", () => {
		const nordLight = lightSchemes.find((scheme) => scheme.id === "nord-light");
		expect(nordLight.system).toBe("base16");
		expect(nordLight.slots).toEqual({
			base00: "#e5e9f0",
			base01: "#c2d0e7",
			base02: "#b8c5db",
			base03: "#aebacf",
			base04: "#60728c",
			base05: "#2e3440",
			base06: "#3b4252",
			base07: "#29838d",
			base08: "#99324b",
			base09: "#ac4426",
			base0A: "#9a7500",
			base0B: "#4f894c",
			base0C: "#398eac",
			base0D: "#3b6ea8",
			base0E: "#97365b",
			base0F: "#5272af",
		});
	});
});
