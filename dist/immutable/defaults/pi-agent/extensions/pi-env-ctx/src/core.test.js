import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import {
	VAR_HARNESS,
	VAR_MODEL,
	VAR_SESSION_ID,
	VAR_SESSION_NAME,
	VAR_VERSION,
	clearOwned,
	exportAll,
	formatModel,
	refreshFromContext,
	updateModel,
	updateSessionScope,
} from "./core.js";

function makeCtx(data = {}) {
	return {
		sessionManager: {
			getSessionId: () => data.sessionId,
			getSessionName: () => data.sessionName,
		},
		model: data.model,
	};
}

describe("formatModel", () => {
	test("returns provider/id for valid model", () => {
		expect(formatModel({ provider: "anthropic", id: "claude-3-7-sonnet" })).toBe("anthropic/claude-3-7-sonnet");
	});

	test("returns undefined for incomplete model", () => {
		expect(formatModel({ provider: "anthropic", id: "" })).toBeUndefined();
		expect(formatModel({ provider: "", id: "claude" })).toBeUndefined();
		expect(formatModel(undefined)).toBeUndefined();
	});
});

describe("exportAll", () => {
	test("maps pi state to AGENT_CTX vars", () => {
		const env = {};
		exportAll(
			makeCtx({
				sessionId: "sess_123",
				sessionName: "my-session",
				model: { provider: "anthropic", id: "claude-3-7-sonnet" },
			}),
			env
		);

		expect(env[VAR_VERSION]).toBe("1");
		expect(env[VAR_HARNESS]).toBe("pi");
		expect(env[VAR_SESSION_ID]).toBe("sess_123");
		expect(env[VAR_MODEL]).toBe("anthropic/claude-3-7-sonnet");
		expect(env[VAR_SESSION_NAME]).toBe("my-session");
	});

	test("leaves unknown fields unset", () => {
		const env = {
			[VAR_SESSION_ID]: "stale-session",
			[VAR_MODEL]: "stale/model",
			[VAR_SESSION_NAME]: "stale-name",
		};
		exportAll(makeCtx(), env);

		expect(env[VAR_VERSION]).toBe("1");
		expect(env[VAR_HARNESS]).toBe("pi");
		expect(env[VAR_SESSION_ID]).toBeUndefined();
		expect(env[VAR_MODEL]).toBeUndefined();
		expect(env[VAR_SESSION_NAME]).toBeUndefined();
	});
});

describe("mutation behavior", () => {
	test("updates model and session name over time", () => {
		const env = {};
		exportAll(
			makeCtx({
				sessionId: "sess_999",
				sessionName: "initial",
				model: { provider: "openai", id: "gpt-4.1" },
			}),
			env
		);

		updateModel({ provider: "anthropic", id: "claude-3-7-sonnet" }, env);
		expect(env[VAR_MODEL]).toBe("anthropic/claude-3-7-sonnet");

		updateSessionScope(makeCtx({ sessionName: "renamed" }), env);
		expect(env[VAR_SESSION_NAME]).toBe("renamed");

		updateModel(undefined, env);
		expect(env[VAR_MODEL]).toBeUndefined();
	});

	test("updates session id when active session changes in the same process", () => {
		const env = {};
		exportAll(makeCtx({ sessionId: "sess_a", sessionName: "a" }), env);
		expect(env[VAR_SESSION_ID]).toBe("sess_a");

		updateSessionScope(makeCtx({ sessionId: "sess_b", sessionName: "b" }), env);
		expect(env[VAR_SESSION_ID]).toBe("sess_b");
		expect(env[VAR_SESSION_NAME]).toBe("b");
	});

	test("refreshFromContext updates session scope and model together", () => {
		const env = {};
		exportAll(makeCtx({ sessionId: "sess_a", sessionName: "a", model: { provider: "openai", id: "gpt-4.1" } }), env);

		refreshFromContext(
			makeCtx({ sessionId: "sess_z", sessionName: "z", model: { provider: "anthropic", id: "claude-3-7-sonnet" } }),
			env
		);
		expect(env[VAR_SESSION_ID]).toBe("sess_z");
		expect(env[VAR_SESSION_NAME]).toBe("z");
		expect(env[VAR_MODEL]).toBe("anthropic/claude-3-7-sonnet");
	});

	test("clearOwned removes all extension-owned vars", () => {
		const env = {};
		exportAll(
			makeCtx({
				sessionId: "sess_clear",
				sessionName: "clear-me",
				model: { provider: "openai", id: "gpt-4.1" },
			}),
			env
		);

		clearOwned(env);
		expect(env[VAR_VERSION]).toBeUndefined();
		expect(env[VAR_HARNESS]).toBeUndefined();
		expect(env[VAR_SESSION_ID]).toBeUndefined();
		expect(env[VAR_MODEL]).toBeUndefined();
		expect(env[VAR_SESSION_NAME]).toBeUndefined();
	});
});

describe("child process propagation", () => {
	test("spawned child process sees AGENT_CTX values", () => {
		const env = {};
		exportAll(
			makeCtx({
				sessionId: "sess_child",
				sessionName: "child-visible",
				model: { provider: "anthropic", id: "claude-3-5-haiku" },
			}),
			env
		);

		const script = `
const keys = [
  "AGENT_CTX_VERSION",
  "AGENT_CTX_HARNESS",
  "AGENT_CTX_HARNESS_SESSION_ID",
  "AGENT_CTX_MODEL",
  "AGENT_CTX_SESSION_NAME",
];
const out = Object.fromEntries(keys.map((k) => [k, process.env[k] ?? null]));
console.log(JSON.stringify(out));
`;
		const result = spawnSync(process.execPath, ["-e", script], {
			env: { ...process.env, ...env },
			encoding: "utf8",
		});

		expect(result.status).toBe(0);
		const payload = JSON.parse(result.stdout.trim());
		expect(payload.AGENT_CTX_VERSION).toBe("1");
		expect(payload.AGENT_CTX_HARNESS).toBe("pi");
		expect(payload.AGENT_CTX_HARNESS_SESSION_ID).toBe("sess_child");
		expect(payload.AGENT_CTX_MODEL).toBe("anthropic/claude-3-5-haiku");
		expect(payload.AGENT_CTX_SESSION_NAME).toBe("child-visible");
	});
});
