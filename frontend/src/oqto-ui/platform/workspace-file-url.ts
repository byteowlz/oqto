/** Same-origin, cookie-authenticated workspace file URL for native viewers. */
export function workspaceFilePreviewUrl(
	workspacePath: string,
	path: string,
): string {
	const url = new URL("/api/workspace/files/file", window.location.origin);
	url.searchParams.set("path", path);
	url.searchParams.set("workspace_path", workspacePath);
	return url.toString();
}

/**
 * The inverse: recover the workspace-relative path from a URL this module
 * built. It lives beside the builder so the two cannot drift; anything that
 * did not come from here is returned unchanged, since a remote or data URL
 * has no workspace path to recover.
 */
export function workspaceFilePathFromUrl(url: string): string {
	try {
		return new URL(url, window.location.origin).searchParams.get("path") ?? url;
	} catch {
		return url;
	}
}
