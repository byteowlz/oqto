/**
 * pi-env-ctx — Export AGENT_CTX_* environment variables for Pi-native session metadata.
 *
 * Injects a small, well-defined set of env vars into process.env so child
 * processes spawned by Pi (tools, bash commands, sub-agents) can identify the
 * harness, session, and model they are running under.
 *
 * Ownership contract (this extension owns ONLY these vars):
 *   AGENT_CTX_VERSION             "1"          — contract version
 *   AGENT_CTX_HARNESS             "pi"         — fixed harness identifier
 *   AGENT_CTX_HARNESS_SESSION_ID  <session id> — Pi session id (authoritative)
 *   AGENT_CTX_MODEL               provider/id  — currently active model
 *   AGENT_CTX_SESSION_NAME        <name>       — display name (mutable, may
 *                                                appear after auto-rename)
 *
 * Out of scope (owned by runner/sandbox, not this extension):
 *   AGENT_CTX_WORKSPACE, AGENT_CTX_PLATFORM_SESSION_ID, AGENT_CTX_USER_ID, etc.
 *
 * Semantics:
 *   - Optional: missing values leave the var UNSET (never an empty string) so
 *     consumers can rely on `process.env.X === undefined` as "unknown".
 *   - Process-local: values are written to process.env and inherited by child
 *     processes via the standard Node spawn behavior.
 *   - Metadata only: do not use these vars as a security boundary.
 */

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { clearOwned, exportAll, refreshFromContext, updateModel, updateSessionScope } from "./src/core.js";

export default function piEnvCtx(pi: ExtensionAPI) {
	pi.on("session_start", (_event, ctx) => {
		exportAll(ctx);
	});

	// Run before the first turn starts so first tool calls can see latest scope/model.
	pi.on("before_agent_start", (_event, ctx) => {
		refreshFromContext(ctx);
	});

	pi.on("turn_start", (_event, ctx) => {
		refreshFromContext(ctx);
	});

	pi.on("model_select", (event, _ctx) => {
		updateModel(event.model);
	});

	pi.on("session_tree", (_event, ctx) => {
		updateSessionScope(ctx);
	});

	pi.on("turn_end", (_event, ctx) => {
		updateSessionScope(ctx);
	});

	pi.on("session_shutdown", () => {
		clearOwned();
	});
}
