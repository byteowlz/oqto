/**
 * Realistic browser defaults.
 *
 * Playwright's default user-agent includes "HeadlessChrome" which is trivially
 * detected.  We ship a recent stable Chrome UA string so that pages see a
 * normal-looking browser out of the box.  Users can still override via
 * AGENT_BROWSER_USER_AGENT or the launch command's `userAgent` field.
 *
 * The accept headers match what a stock Chrome installation sends on every
 * navigation and sub-resource request.
 */

import os from "node:os";

// --- User-Agent -----------------------------------------------------------

function platformToken(): string {
  const platform = os.platform();
  const arch = os.arch();

  if (platform === "darwin") {
    // macOS reports arm64 or x64
    const macArch = arch === "arm64" ? "ARM64" : "Intel";
    return `Macintosh; ${macArch} Mac OS X 10_15_7`;
  }

  if (platform === "win32") {
    return "Windows NT 10.0; Win64; x64";
  }

  // Linux / other
  return "X11; Linux x86_64";
}

/**
 * Return a realistic Chrome user-agent string for the current platform.
 *
 * The version numbers are pinned to a recent stable Chrome release.  Bump
 * these periodically to stay current.
 */
export function defaultUserAgent(): string {
  const token = platformToken();
  // Chrome 131 stable (Jan 2025 baseline -- close enough for most sites)
  return (
    `Mozilla/5.0 (${token}) ` +
    "AppleWebKit/537.36 (KHTML, like Gecko) " +
    "Chrome/131.0.0.0 Safari/537.36"
  );
}

// --- Accept headers -------------------------------------------------------

/**
 * Standard request headers that Chrome sends on navigation requests.
 * Setting these avoids the tell-tale Playwright defaults (missing
 * Accept-Language, etc.).
 */
export function defaultHeaders(): Record<string, string> {
  return {
    "Accept-Language": "en-US,en;q=0.9",
    // sec-ch-ua hints that match the UA string above
    "sec-ch-ua": '"Chromium";v="131", "Not_A Brand";v="24"',
    "sec-ch-ua-mobile": "?0",
    "sec-ch-ua-platform": `"${defaultPlatformHint()}"`,
  };
}

function defaultPlatformHint(): string {
  const platform = os.platform();
  if (platform === "darwin") return "macOS";
  if (platform === "win32") return "Windows";
  return "Linux";
}
