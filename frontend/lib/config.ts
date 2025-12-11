const trimTrailingSlash = (value?: string) => value?.replace(/\/$/, "") ?? ""

export const appConfig = {
  opencodeBaseUrl: trimTrailingSlash(process.env.NEXT_PUBLIC_OPENCODE_BASE_URL),
  terminalWsUrl: trimTrailingSlash(process.env.NEXT_PUBLIC_TERMINAL_WS_URL),
  fileServerBaseUrl: trimTrailingSlash(process.env.NEXT_PUBLIC_FILE_SERVER_URL),
}

export const hasOpencode = Boolean(appConfig.opencodeBaseUrl)
export const hasTerminal = Boolean(appConfig.terminalWsUrl)
export const hasFileServer = Boolean(appConfig.fileServerBaseUrl)
