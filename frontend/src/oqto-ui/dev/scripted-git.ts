/**
 * Scripted version control for /dev/oqto-ui: a clean tree. The dev route
 * carries no invented repository state; the pane's own empty state is what
 * it exercises.
 */

import type { GitHost } from "../platform/git-contract";

export function createScriptedGitHost(): GitHost {
	return {
		async status() {
			return {
				branch: "main",
				upstream: null,
				ahead: 0,
				behind: 0,
				entries: [],
				truncated: false,
			};
		},
		async diff(_workspacePath, path, staged) {
			return { path, staged, patch: "", truncated: false };
		},
		async log() {
			return [];
		},
		async stage() {},
		async commit() {
			return "";
		},
	};
}
