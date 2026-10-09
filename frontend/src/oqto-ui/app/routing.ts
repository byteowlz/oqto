import type { UiNavigation } from "../platform/contracts";

export function applyNavigation(
	current: URLSearchParams,
	next: UiNavigation,
): URLSearchParams {
	const updated = new URLSearchParams(current);
	if (next.workDirectoryId) updated.set("workDirectory", next.workDirectoryId);
	if (next.sessionId) updated.set("session", next.sessionId);
	if (next.mobileView) updated.set("view", next.mobileView);
	if (next.schemeId) updated.set("scheme", next.schemeId);
	if (next.workAreaTab) updated.set("tab", next.workAreaTab);
	return updated;
}
