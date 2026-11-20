import type { ChatPresentation } from "@/lib/chat-rendering/CanonicalMessageRenderer";
import { OqtoIcon, type OqtoIconName } from "../platform/icons";

// OperationGlyph's selected native families; presentation only, never tool policy.
function operationIcon(name: string): OqtoIconName {
	const token = name.toLowerCase();
	if (token.includes("edit") || token.includes("write")) return "edit";
	if (token.includes("search") || token.includes("grep")) return "search";
	if (token.includes("bash") || token.includes("shell")) return "terminal";
	if (token.includes("read") || token.includes("file")) return "file";
	return "settings";
}

export const compactChatPresentation: ChatPresentation = {
	style: "compact",
	icons: {
		copy: <OqtoIcon name="copy" />,
		copied: <OqtoIcon name="check" />,
		chevron: <OqtoIcon name="forward" />,
		operation: (name, failed) => (
			<OqtoIcon name={failed ? "info" : operationIcon(name)} />
		),
	},
};
