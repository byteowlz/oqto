import { describeAgentError } from "@/features/chat/utils/agent-error";
import { describe, expect, it } from "vitest";

/**
 * Every input here is a failure that actually reached a user, each of which
 * originally surfaced as an unrelated model selection timeout.
 */
describe("describeAgentError", () => {
	it("names the denied path, because the path is the whole diagnosis", () => {
		const result = describeAgentError(
			`Agent process exited. Last stderr output:
EACCES: permission denied, mkdir '/home/u/.claude/projects/-home-u-byteowlz-Teachme'`,
		);

		expect(result.kind).toBe("sandbox-denied");
		expect(result.headline).toContain(
			"/home/u/.claude/projects/-home-u-byteowlz-Teachme",
		);
	});

	it("reports an allocation failure rather than the frame it happened in", () => {
		const result = describeAgentError(
			`Agent process exited. Last stderr output:
node:internal/deps/undici/undici:5971
      return await WebAssembly.instantiate(mod, {
                               ^
RangeError: WebAssembly.instantiate(): Out of memory: Cannot allocate Wasm memory for new instance
    at lazyllhttp (node:internal/deps/undici/undici:5971:32)
Node.js v22.22.2`,
		);

		expect(result.kind).toBe("allocation-failed");
		expect(result.cause).toContain("Cannot allocate Wasm memory");
		// The chosen line must not be a stack frame or the echoed source.
		expect(result.cause).not.toMatch(/^\s*at\s/);
		expect(result.cause).not.toContain("return await");
	});

	it("distinguishes an unreachable service from a crash", () => {
		const result = describeAgentError(
			`Agent process exited. Last stderr output:
Error: connect ECONNREFUSED 127.0.0.1:3033`,
		);

		expect(result.kind).toBe("unreachable");
	});

	it("still explains an exit it cannot classify", () => {
		const result = describeAgentError(
			`Agent process exited. Last stderr output:
Killed`,
		);

		expect(result.kind).toBe("exited");
		expect(result.headline).toBe("The agent stopped unexpectedly.");
	});

	it("keeps a short message as the headline without duplicating it below", () => {
		const result = describeAgentError("Model provider rejected the request");

		expect(result.headline).toBe("Model provider rejected the request");
		expect(result.detail).toBeUndefined();
		expect(result.cause).toBeUndefined();
	});

	it("offers detail only when there is more to see than the headline", () => {
		const bare = describeAgentError("Agent process exited");
		expect(bare.detail).toBeUndefined();

		const withOutput = describeAgentError(
			`Agent process exited. Last stderr output:
Killed`,
		);
		expect(withOutput.detail).toBe("Killed");
	});

	it("does not throw on empty input", () => {
		expect(describeAgentError("").kind).toBe("generic");
	});
});
