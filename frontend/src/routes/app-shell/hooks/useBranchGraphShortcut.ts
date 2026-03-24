import { useMountEffect } from "@/hooks/use-mount-effect";

export function useBranchGraphShortcut(onOpen: () => void): void {
	useMountEffect(() => {
		window.addEventListener("oqto:open-branch-graph", onOpen);
		return () => window.removeEventListener("oqto:open-branch-graph", onOpen);
	});
}
