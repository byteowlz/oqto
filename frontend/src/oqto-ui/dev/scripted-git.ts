/**
 * Scripted version control for /dev/oqto-ui: a clean tree. The dev route
 * carries no invented repository state; the pane's own empty state is what
 * it exercises.
 */

import type { GitHost } from "../platform/git-contract";

/** One small patch, so the dev route exercises the renderer, not an empty box. */
const PATCH = [
	"diff --git a/greeting.ts b/greeting.ts",
	"index 1111111..2222222 100644",
	"--- a/greeting.ts",
	"+++ b/greeting.ts",
	"@@ -1,4 +1,4 @@",
	"export function greeting(name: string): string {",
	"-\treturn `Hello, ${name}`;",
	"+\treturn `Hello, ${name}!`;",
	"}",
	"",
].join("\n");

export function createScriptedGitHost(): GitHost {
	return {
		async status() {
			return {
				branch: "main",
				upstream: null,
				ahead: 0,
				behind: 0,
				entries: [
					{
						path: "greeting.ts",
						index: " ",
						worktree: "M",
						renamedFrom: null,
					},
				],
				truncated: false,
			};
		},
		async diff(_workspacePath, path, staged) {
			return { path, staged, patch: PATCH, truncated: false };
		},
		async log() {
			return [];
		},
		async stage() {},
		branches: {
			async list() {
				return [{ name: "main", current: true, worktree: null }];
			},
			async switch() {},
		},
		async remote(_workspacePath, operation) {
			return { operation, summary: "" };
		},
		async commit() {
			return "";
		},
	};
}
