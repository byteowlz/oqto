import { useEffect } from "react";

interface UseGodmodeShortcutInput {
	activateGodmode: () => void;
	onboardingCompleted: boolean;
	onboardingGodmode: boolean;
}

export function useGodmodeShortcut({
	activateGodmode,
	onboardingCompleted,
	onboardingGodmode,
}: UseGodmodeShortcutInput): void {
	// useeffect-guardrail: allow - document keyboard shortcut subscription
	useEffect(() => {
		const handleKeyDown = (event: KeyboardEvent) => {
			if (
				event.key === "g" &&
				(event.metaKey || event.ctrlKey) &&
				event.shiftKey
			) {
				event.preventDefault();
				if (!onboardingCompleted && !onboardingGodmode) {
					activateGodmode();
				}
			}
		};

		document.addEventListener("keydown", handleKeyDown);
		return () => document.removeEventListener("keydown", handleKeyDown);
	}, [activateGodmode, onboardingCompleted, onboardingGodmode]);
}
