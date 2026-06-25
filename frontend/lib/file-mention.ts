/**
 * Compute the result of inserting an `@<path>` file mention into composer text
 * at a cursor position. Pure (no DOM) so it can be unit-tested; the DOM glue
 * (focus, setSelectionRange) lives in the composer.
 *
 * The inserted token is `@<path> ` (a trailing space), so the cursor advances
 * by `path.length + 2` (the `@` and the space).
 */
export function insertFileMention(
	value: string,
	cursor: number,
	path: string,
): { value: string; cursor: number } {
	const at = Math.max(0, Math.min(cursor, value.length));
	const nextValue = `${value.slice(0, at)}@${path} ${value.slice(at)}`;
	return { value: nextValue, cursor: at + path.length + 2 };
}
