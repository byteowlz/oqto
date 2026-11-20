import { ProviderIcon } from "@/components/data-display/provider-icon";
import { providerIconUrl } from "@/components/data-display/provider-icons/provider-icons";
import catalog from "@/components/data-display/provider-icons/provider-icons-catalog.json";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

describe("pinned offline provider branding", () => {
	it("resolves every canonical alias and never guesses a brand from a path, model or gateway", () => {
		for (const [alias, brand] of Object.entries(catalog.aliases)) {
			expect(providerIconUrl(alias)).toBe(providerIconUrl(brand));
			expect(providerIconUrl(alias)).toBeTruthy();
		}
		expect(providerIconUrl(" OpenAI-Codex ")).toBe(providerIconUrl("openai"));
		for (const unknown of [
			"custom-provider",
			"gateway/openai",
			"openai-compatible",
			"__proto__",
			"constructor",
			"../openai",
			"claude-opus",
			"ｏｐｅｎａｉ",
			"",
		])
			expect(providerIconUrl(unknown)).toBeUndefined();
	});
	it("inherits arbitrary theme ink without a remote image or inversion", () => {
		render(<ProviderIcon provider="anthropic" />);
		const icon = screen.getByRole("img", { name: "anthropic" });
		expect(icon.tagName).toBe("SPAN");
		expect(icon.style.backgroundColor).toBe("currentcolor");
		expect(icon.style.maskImage).toBe(`url("${providerIconUrl("anthropic")}")`);
		expect(icon.outerHTML).not.toContain("githubusercontent");
		expect(icon.className).not.toContain("invert");
	});
	it("updates identity immediately across known, unknown and known providers", () => {
		const { rerender } = render(<ProviderIcon provider="unknown-custom" />);
		expect(screen.getByRole("img").tagName.toLowerCase()).toBe("svg");
		rerender(<ProviderIcon provider="openai" />);
		expect(screen.getByRole("img", { name: "openai" }).style.maskImage).toBe(
			`url("${providerIconUrl("openai")}")`,
		);
		rerender(<ProviderIcon provider="google" />);
		expect(screen.getByRole("img", { name: "google" }).style.maskImage).toBe(
			`url("${providerIconUrl("google")}")`,
		);
		rerender(<ProviderIcon provider="custom-provider" />);
		expect(
			screen
				.getByRole("img", { name: "custom-provider" })
				.tagName.toLowerCase(),
		).toBe("svg");
	});
});
