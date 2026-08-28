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
