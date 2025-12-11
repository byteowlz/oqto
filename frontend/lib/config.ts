const trimTrailingSlash = (value?: string) => value?.replace(/\/$/, "") ?? ""

// Use proxy path to avoid CORS issues, fall back to direct URL for SSR
const getOpencodeUrl = () => {
  if (typeof window !== "undefined") {
    // Client-side: use the proxy
    return "/api/opencode"
  }
  // Server-side: use the direct URL
  return trimTrailingSlash(process.env.NEXT_PUBLIC_OPENCODE_BASE_URL)
}

export const appConfig = {
  opencodeBaseUrl: getOpencodeUrl(),
  terminalWsUrl: trimTrailingSlash(process.env.NEXT_PUBLIC_TERMINAL_WS_URL),
  fileServerBaseUrl: trimTrailingSlash(process.env.NEXT_PUBLIC_FILE_SERVER_URL),
}

export const hasOpencode = Boolean(process.env.NEXT_PUBLIC_OPENCODE_BASE_URL)
export const hasTerminal = Boolean(appConfig.terminalWsUrl)
export const hasFileServer = Boolean(appConfig.fileServerBaseUrl)
