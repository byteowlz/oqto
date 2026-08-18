import type { OqtoUiPlatform, OqtoUiSnapshot } from "../platform/contracts";
import { scriptedFixture } from "./fixture";

export const scriptedOqtoUiPlatform: OqtoUiPlatform = {
	async load(sessionId): Promise<OqtoUiSnapshot> {
		const known = scriptedFixture.workDirectories.some((directory) =>
			directory.sessions.some((session) => session.id === sessionId),
		);
		const activeSessionId = known
			? sessionId
			: (scriptedFixture.workDirectories[0]?.sessions[0]?.id ?? null);
		return { ...scriptedFixture, activeSessionId };
	},
};
