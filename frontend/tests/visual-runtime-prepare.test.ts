import { describe, expect, it } from "vitest";

import { prepareVisualRuntimeDocument } from "@/features/sessions/visual-runtime";

describe("prepareVisualRuntimeDocument", () => {
	it("rewrites known remote script URLs to local bundles in offline_strict", () => {
		const input = `
      <html><head>
        <script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
      </head><body><div class="mermaid">graph TD;A-->B;</div></body></html>
    `;

		const result = prepareVisualRuntimeDocument({
			html: input,
			mode: "offline_strict",
		});

		expect(result.html).toContain("/visual-runtime/vendor/mermaid.min.js");
		expect(result.requires).toContain("mermaid");
		expect(result.diagnostics.some((d) => d.level === "info")).toBe(true);
	});

	it("blocks unknown remote scripts in offline_strict", () => {
		const input = `<html><head><script src="https://evil.example/evil.js"></script></head><body></body></html>`;
		const result = prepareVisualRuntimeDocument({
			html: input,
			mode: "offline_strict",
		});

		expect(result.html).not.toContain("evil.example");
		expect(
			result.diagnostics.some((d) =>
				d.message.includes("Blocked remote script"),
			),
		).toBe(true);
	});

	it("injects CSP meta policy", () => {
		const result = prepareVisualRuntimeDocument({
			html: "<html><head></head><body><h1>ok</h1></body></html>",
			mode: "offline_strict",
		});
		expect(result.html).toContain("Content-Security-Policy");
		expect(result.html).toContain("default-src 'none'");
		expect(result.html).toContain("script-src 'self' 'nonce-");
		expect(result.html).not.toContain("'unsafe-inline'");
	});

	it("applies CSP nonce to inline scripts and styles", () => {
		const result = prepareVisualRuntimeDocument({
			html: "<html><head><style>body{color:red}</style></head><body><script>console.log('x')</script></body></html>",
			mode: "offline_strict",
		});

		expect(result.html).toMatch(/<script nonce="[a-f0-9]+">/);
		expect(result.html).toMatch(/<style nonce="[a-f0-9]+">/);
	});
});
