import { describe, expect, test } from "bun:test";
import piEnvCtx from "./index.ts";

function createPiStub() {
	const handlers = new Map();
	return {
		on(event, handler) {
			handlers.set(event, handler);
		},
		handlers,
	};
}

function makeCtx({ sessionId, sessionName, model } = {}) {
	return {
		sessionManager: {
			getSessionId: () => sessionId,
			getSessionName: () => sessionName,
		},
		model,
	};
}

describe("pi-env-ctx event wiring", () => {
	test("registers expected handlers and mutates env on lifecycle events", () => {
		const pi = createPiStub();
		piEnvCtx(pi);

		expect(pi.handlers.has("session_start")).toBe(true);
		expect(pi.handlers.has("before_agent_start")).toBe(true);
		expect(pi.handlers.has("turn_start")).toBe(true);
		expect(pi.handlers.has("model_select")).toBe(true);
		expect(pi.handlers.has("session_tree")).toBe(true);
		expect(pi.handlers.has("turn_end")).toBe(true);
		expect(pi.handlers.has("session_shutdown")).toBe(true);

		const prev = {
			AGENT_CTX_VERSION: process.env.AGENT_CTX_VERSION,
			AGENT_CTX_HARNESS: process.env.AGENT_CTX_HARNESS,
			AGENT_CTX_HARNESS_SESSION_ID: process.env.AGENT_CTX_HARNESS_SESSION_ID,
			AGENT_CTX_MODEL: process.env.AGENT_CTX_MODEL,
			AGENT_CTX_SESSION_NAME: process.env.AGENT_CTX_SESSION_NAME,
		};

		const restore = () => {
			for (const [k, v] of Object.entries(prev)) {
				if (v === undefined) delete process.env[k];
				else process.env[k] = v;
			}
		};

		try {
			pi.handlers.get("session_start")(
				{ type: "session_start", reason: "startup" },
				makeCtx({
					sessionId: "sess_wired",
					sessionName: "wired",
					model: { provider: "anthropic", id: "claude-3-7-sonnet" },
				})
			);

			expect(process.env.AGENT_CTX_VERSION).toBe("1");
			expect(process.env.AGENT_CTX_HARNESS).toBe("pi");
			expect(process.env.AGENT_CTX_HARNESS_SESSION_ID).toBe("sess_wired");
			expect(process.env.AGENT_CTX_MODEL).toBe("anthropic/claude-3-7-sonnet");
			expect(process.env.AGENT_CTX_SESSION_NAME).toBe("wired");

			pi.handlers.get("model_select")({ type: "model_select", model: { provider: "openai", id: "gpt-4.1" } }, makeCtx());
			expect(process.env.AGENT_CTX_MODEL).toBe("openai/gpt-4.1");

			pi.handlers.get("before_agent_start")(
				{ type: "before_agent_start", prompt: "hi" },
				makeCtx({
					sessionId: "sess_early",
					sessionName: "early-name",
					model: { provider: "anthropic", id: "claude-3-5-haiku" },
				})
			);
			expect(process.env.AGENT_CTX_HARNESS_SESSION_ID).toBe("sess_early");
			expect(process.env.AGENT_CTX_SESSION_NAME).toBe("early-name");
			expect(process.env.AGENT_CTX_MODEL).toBe("anthropic/claude-3-5-haiku");

			pi.handlers.get("session_shutdown")({ type: "session_shutdown", reason: "quit" }, makeCtx());
			expect(process.env.AGENT_CTX_VERSION).toBeUndefined();
			expect(process.env.AGENT_CTX_HARNESS).toBeUndefined();
			expect(process.env.AGENT_CTX_HARNESS_SESSION_ID).toBeUndefined();
			expect(process.env.AGENT_CTX_MODEL).toBeUndefined();
			expect(process.env.AGENT_CTX_SESSION_NAME).toBeUndefined();
		} finally {
			restore();
		}
	});
});
