import { AgentErrorBody } from "@/features/chat/components/AgentErrorBody";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

describe("AgentErrorBody", () => {
	const crash = `Agent process exited. Last stderr output:
node:internal/deps/undici/undici:5971
      return await WebAssembly.instantiate(mod, {
                               ^
RangeError: WebAssembly.instantiate(): Out of memory: Cannot allocate Wasm memory for new instance
    at lazyllhttp (node:internal/deps/undici/undici:5971:32)
Node.js v22.22.2`;

	it("leads with the headline and keeps the trace collapsed", () => {
		render(<AgentErrorBody text={crash} />);

		expect(
			screen.getByText("The agent could not allocate memory and stopped."),
		).toBeTruthy();
		// The stack is present for whoever wants it, but inside a closed disclosure.
		const details = document.querySelector("details");
		expect(details).toBeTruthy();
		expect((details as HTMLDetailsElement).open).toBe(false);
		expect(document.body.textContent).toContain("lazyllhttp");
	});

	it("shows no disclosure when a short message says everything", () => {
		render(<AgentErrorBody text="Model provider rejected the request" />);
		expect(document.querySelector("details")).toBeNull();
	});
});
