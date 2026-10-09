/**
 * Scripted issue tracker for /dev/oqto-ui. The dev route carries no invented
 * issues: fixture titles would be untranslated product text, and an empty
 * tracker still exercises the pane's own empty state.
 */

import type { IssueHost } from "../platform/issues-contract";

export function createScriptedIssueHost(): IssueHost {
	return {
		async list() {
			return [];
		},
		async setStatus() {},
	};
}
