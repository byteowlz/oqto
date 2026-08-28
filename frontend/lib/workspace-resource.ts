/** Normalize a file reference to a path relative to its Work Directory. */
export function normalizeWorkspaceFileReference(
	reference: string,
	workspacePath?: string | null,
): string {
	const withoutMarker = reference.trim().replace(/^@+/, "");
	const withoutScheme = withoutMarker.replace(/^file:\/\//, "");
	const root = (workspacePath ?? "").replace(/\/+$/, "");
	const relative =
		root && (withoutScheme === root || withoutScheme.startsWith(`${root}/`))
			? withoutScheme.slice(root.length).replace(/^\/+/, "")
			: withoutScheme;
	return relative.replace(/^\.\//, "");
}
