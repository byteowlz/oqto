const trimTrailingSlash = (value?: string) => value?.replace(/\/$/, "") ?? "";

const env =
	(import.meta as ImportMeta & { env?: Record<string, string | undefined> })
		.env ?? (typeof process !== "undefined" ? process.env : {});

// Get the base Caddy URL for container routing
const getCaddyBaseUrl = () => {
	return trimTrailingSlash(env.VITE_CADDY_BASE_URL) || "";
};

// Build container-specific URLs
// containerId is optional - if not provided, falls back to legacy direct URLs
export const getContainerUrls = (containerId?: string) => {
	const caddyBase = getCaddyBaseUrl();

	if (containerId && caddyBase) {
		// Route through Caddy with container-specific paths
		const basePath = `/c/${containerId}`;
		return {
			agentBaseUrl: `${basePath}/api`,
			fileServerBaseUrl: `${basePath}/files`,
			terminalWsUrl: `${caddyBase.replace(/^http/, "ws")}${basePath}/term`,
		};
	}

	// Fallback to legacy direct URLs (local dev without Caddy)
	return {
		agentBaseUrl:
			typeof window !== "undefined"
				? "/api/agent"
				: trimTrailingSlash(env.VITE_AGENT_BASE_URL),
		fileServerBaseUrl: trimTrailingSlash(env.VITE_FILE_SERVER_URL),
		terminalWsUrl: trimTrailingSlash(env.VITE_TERMINAL_WS_URL),
	};
};

// Legacy config for backwards compatibility (uses default/no container)
export const appConfig = {
	...getContainerUrls(),
	caddyBaseUrl: getCaddyBaseUrl(),
};

// Feature flags
export const hasOpencode = Boolean(
	env.VITE_OPENCODE_BASE_URL || env.VITE_CADDY_BASE_URL,
);
export const hasTerminal = Boolean(
	env.VITE_TERMINAL_WS_URL || env.VITE_CADDY_BASE_URL,
);
export const hasFileServer = Boolean(
	env.VITE_FILE_SERVER_URL || env.VITE_CADDY_BASE_URL,
);
export const hasCaddy = Boolean(env.VITE_CADDY_BASE_URL);
