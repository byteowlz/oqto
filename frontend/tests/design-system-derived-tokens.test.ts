import {
	MANAGED_SEMANTIC_VARS,
	mapSchemeToTokens,
	oqtoDark,
} from "@byteowlz/design-system";
import { describe, expect, it } from "vitest";

describe("derived semantic token tier", () => {
	const tokens = mapSchemeToTokens(oqtoDark);

	it("emits the derived text/state shades computed from base vars", () => {
		for (const name of [
			"--subtle-foreground",
			"--readback-foreground",
			"--state-hover",
			"--state-active",
		] as const) {
			expect(tokens[name]).toMatch(/^color-mix\(/);
		}
	});

	it("derives from the base semantic vars so they adapt to the active mode", () => {
		expect(tokens["--subtle-foreground"]).toContain("var(--foreground)");
		expect(tokens["--subtle-foreground"]).toContain("var(--background)");
		expect(tokens["--readback-foreground"]).toContain("var(--foreground)");
		expect(tokens["--state-hover"]).toContain("var(--muted)");
		expect(tokens["--state-active"]).toContain("var(--muted)");
	});

	it("registers the derived tokens so clearScheme clears them", () => {
		for (const name of [
			"--subtle-foreground",
			"--readback-foreground",
			"--state-hover",
			"--state-active",
		] as const) {
			expect(MANAGED_SEMANTIC_VARS).toContain(name);
		}
	});
});
