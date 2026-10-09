import { useDocumentEvent } from "@/hooks/use-document-event";
import type { OqtoUiConfig, UiNavigation } from "../platform/contracts";

const supportedActions: Readonly<
	Record<string, (navigate: (next: UiNavigation) => void) => void>
> = {
	"view.openFiles": (navigate) => navigate({ mobileView: "files" }),
	"view.openChat": (navigate) => navigate({ mobileView: "chat" }),
};

function chord(event: KeyboardEvent): string {
	return [
		event.ctrlKey ? "ctrl" : "",
		event.altKey ? "alt" : "",
		event.shiftKey ? "shift" : "",
		event.metaKey ? "meta" : "",
		event.key.toLowerCase(),
	]
		.filter(Boolean)
		.join("+");
}

export function useConfiguredBindings(
	bindings: OqtoUiConfig["bindings"],
	onNavigate: (next: UiNavigation) => void,
): void {
	useDocumentEvent("keydown", (event) => {
		if (event.repeat) return;
		const binding = bindings.find(
			(candidate) => candidate.keys.toLowerCase() === chord(event),
		);
		if (!binding) return;
		const execute = supportedActions[binding.action];
		if (!execute) return;
		event.preventDefault();
		execute(onNavigate);
	});
}
