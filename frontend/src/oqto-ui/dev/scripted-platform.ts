import type { OqtoUiPlatform, OqtoUiSnapshot } from "../platform/contracts";

const GALLERY_SESSION_TITLE = "Gallery App vertical slice";
const REVIEW_SESSION_TITLE = "Review image outputs";

const scenario: OqtoUiSnapshot = {
	sessions: [
		{
			id: "oqto-demo-gallery",
			title: GALLERY_SESSION_TITLE,
			workspace: "agents/coder",
			updatedAt: 1_786_406_400_000,
			model: "anthropic/claude-sonnet-4",
		},
		{
			id: "oqto-demo-review",
			title: REVIEW_SESSION_TITLE,
			workspace: "agents/research",
			updatedAt: 1_786_402_800_000,
			model: "openai/gpt-5",
		},
	],
	activeSessionId: "oqto-demo-gallery",
	timeline: [
		{
			id: "entry-user-1",
			role: "user",
			text: "Build a Gallery App from the images in outputs/.",
			status: "committed",
		},
		{
			id: "entry-agent-1",
			role: "assistant",
			text: "Created apps/gallery/ with a validated manifest and an image opener contribution.",
			status: "committed",
		},
		{
			id: "entry-tool-1",
			role: "tool",
			text: "Validated oqto.app.gallery · definition draft 7c42a1",
			status: "committed",
		},
	],
	gallery: ["cover", "detail", "mobile", "contact-sheet"].map(
		(name, index) => ({
			id: `resource-${index + 1}`,
			name: `${name}.svg`,
			src: "/icons/IMAGE_white.svg",
			width: 320,
			height: 240,
			revision: "sha256:7c42a1",
		}),
	),
	connection: "scripted",
};

export const scriptedOqtoUiPlatform: OqtoUiPlatform = {
	async load(requestedSessionId) {
		const activeSessionId = scenario.sessions.some(
			(session) => session.id === requestedSessionId,
		)
			? requestedSessionId
			: scenario.activeSessionId;
		return { ...scenario, activeSessionId };
	},
};
