import { useEffect } from "react";

interface UseAppShellProjectEventsInput {
	setSelectedProjectKey: (projectKey: string | null) => void;
	setActiveAppId: (appId: string) => void;
	onProjectDefaultAgentChange: (projectKey: string, agentId: string) => void;
}

export function useAppShellProjectEvents({
	setSelectedProjectKey,
	setActiveAppId,
	onProjectDefaultAgentChange,
}: UseAppShellProjectEventsInput): void {
	// useeffect-guardrail: allow - window custom event subscriptions for project controls
	useEffect(() => {
		if (typeof window === "undefined") return;

		const handleFilter = (event: Event) => {
			const customEvent = event as CustomEvent<string>;
			if (typeof customEvent.detail === "string") {
				setSelectedProjectKey(customEvent.detail);
				setActiveAppId("sessions");
			}
		};
		const handleClear = () => setSelectedProjectKey(null);
		const handleDefaultAgent = (event: Event) => {
			const customEvent = event as CustomEvent<{
				projectKey: string;
				agentId: string;
			}>;
			if (!customEvent.detail) return;
			onProjectDefaultAgentChange(
				customEvent.detail.projectKey,
				customEvent.detail.agentId,
			);
		};

		window.addEventListener(
			"oqto:project-filter",
			handleFilter as EventListener,
		);
		window.addEventListener("oqto:project-filter-clear", handleClear);
		window.addEventListener(
			"oqto:project-default-agent",
			handleDefaultAgent as EventListener,
		);
		return () => {
			window.removeEventListener(
				"oqto:project-filter",
				handleFilter as EventListener,
			);
			window.removeEventListener("oqto:project-filter-clear", handleClear);
			window.removeEventListener(
				"oqto:project-default-agent",
				handleDefaultAgent as EventListener,
			);
		};
	}, [onProjectDefaultAgentChange, setActiveAppId, setSelectedProjectKey]);
}
