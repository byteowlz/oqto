const trimTrailingSlash = (value?: string) => value?.replace(/\/$/, "") ?? ""

// Get the base Caddy URL for container routing
const getCaddyBaseUrl = () => {
  return trimTrailingSlash(process.env.NEXT_PUBLIC_CADDY_BASE_URL) || ""
}

// Build container-specific URLs
// containerId is optional - if not provided, falls back to legacy direct URLs
export const getContainerUrls = (containerId?: string) => {
  const caddyBase = getCaddyBaseUrl()
  
  if (containerId && caddyBase) {
    // Route through Caddy with container-specific paths
    const basePath = `/c/${containerId}`
    return {
      opencodeBaseUrl: `${basePath}/api`,
      fileServerBaseUrl: `${basePath}/files`,
      terminalWsUrl: `${caddyBase.replace(/^http/, 'ws')}${basePath}/term`,
    }
  }
  
  // Fallback to legacy direct URLs (local dev without Caddy)
  return {
    opencodeBaseUrl: typeof window !== "undefined" 
      ? "/api/opencode" 
      : trimTrailingSlash(process.env.NEXT_PUBLIC_OPENCODE_BASE_URL),
    fileServerBaseUrl: trimTrailingSlash(process.env.NEXT_PUBLIC_FILE_SERVER_URL),
    terminalWsUrl: trimTrailingSlash(process.env.NEXT_PUBLIC_TERMINAL_WS_URL),
  }
}

// Legacy config for backwards compatibility (uses default/no container)
export const appConfig = {
  ...getContainerUrls(),
  caddyBaseUrl: getCaddyBaseUrl(),
}

// Feature flags
export const hasOpencode = Boolean(process.env.NEXT_PUBLIC_OPENCODE_BASE_URL || process.env.NEXT_PUBLIC_CADDY_BASE_URL)
export const hasTerminal = Boolean(process.env.NEXT_PUBLIC_TERMINAL_WS_URL || process.env.NEXT_PUBLIC_CADDY_BASE_URL)
export const hasFileServer = Boolean(process.env.NEXT_PUBLIC_FILE_SERVER_URL || process.env.NEXT_PUBLIC_CADDY_BASE_URL)
export const hasCaddy = Boolean(process.env.NEXT_PUBLIC_CADDY_BASE_URL)
