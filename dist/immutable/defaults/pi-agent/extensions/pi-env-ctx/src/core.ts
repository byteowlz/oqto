import type { ExtensionContext } from "@earendil-works/pi-coding-agent";

export const CTX_VERSION = "1";
export const HARNESS = "pi";

export const VAR_VERSION = "AGENT_CTX_VERSION";
export const VAR_HARNESS = "AGENT_CTX_HARNESS";
export const VAR_SESSION_ID = "AGENT_CTX_HARNESS_SESSION_ID";
export const VAR_MODEL = "AGENT_CTX_MODEL";
export const VAR_SESSION_NAME = "AGENT_CTX_SESSION_NAME";

export type EnvModel = {
	provider?: string;
	id?: string;
};

function normalizeNonEmpty(value: string | undefined): string | undefined {
	if (typeof value !== "string") return undefined;
	const trimmed = value.trim();
	return trimmed.length > 0 ? trimmed : undefined;
}

export function setOrUnset(name: string, value: string | undefined, env: NodeJS.ProcessEnv = process.env): void {
	if (value) {
		env[name] = value;
		return;
	}
	delete env[name];
}

export function formatModel(model: EnvModel | undefined): string | undefined {
	if (!model) return undefined;
	const provider = normalizeNonEmpty(model.provider);
	const id = normalizeNonEmpty(model.id);
	if (!provider || !id) return undefined;
	return `${provider}/${id}`;
}

export function readSessionId(ctx: ExtensionContext): string | undefined {
	return normalizeNonEmpty(ctx.sessionManager.getSessionId?.());
}

export function readSessionName(ctx: ExtensionContext): string | undefined {
	return normalizeNonEmpty(ctx.sessionManager.getSessionName?.());
}

export function exportAll(ctx: ExtensionContext, env: NodeJS.ProcessEnv = process.env): void {
	env[VAR_VERSION] = CTX_VERSION;
	env[VAR_HARNESS] = HARNESS;
	setOrUnset(VAR_SESSION_ID, readSessionId(ctx), env);
	setOrUnset(VAR_MODEL, formatModel(ctx.model), env);
	setOrUnset(VAR_SESSION_NAME, readSessionName(ctx), env);
}

export function updateModel(model: EnvModel | undefined, env: NodeJS.ProcessEnv = process.env): void {
	setOrUnset(VAR_MODEL, formatModel(model), env);
}

export function updateSessionScope(ctx: ExtensionContext, env: NodeJS.ProcessEnv = process.env): void {
	setOrUnset(VAR_SESSION_ID, readSessionId(ctx), env);
	setOrUnset(VAR_SESSION_NAME, readSessionName(ctx), env);
}

/**
 * Refresh all mutable context fields from the latest Pi runtime view.
 * Useful right before a turn or tool activity so first writes carry fresh ctx.
 */
export function refreshFromContext(ctx: ExtensionContext, env: NodeJS.ProcessEnv = process.env): void {
	updateSessionScope(ctx, env);
	updateModel(ctx.model, env);
}

/**
 * Clear all vars owned by this extension.
 * Useful on session/runtime shutdown to avoid leaking stale metadata.
 */
export function clearOwned(env: NodeJS.ProcessEnv = process.env): void {
	delete env[VAR_VERSION];
	delete env[VAR_HARNESS];
	delete env[VAR_SESSION_ID];
	delete env[VAR_MODEL];
	delete env[VAR_SESSION_NAME];
}
