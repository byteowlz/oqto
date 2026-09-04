import type { AppInstanceSummary } from "@/src/generated/AppInstanceSummary";
import type { AppPresentationDocument } from "@/src/generated/AppPresentationDocument";
import type { AppTab } from "./components/AppView";

export interface OpenedAppTabs {
	tabs: AppTab[];
	activeTabId: string;
}

/** Replace a superseded App Instance in place instead of opening a peer tab. */
export function openOqtoAppTab(
	tabs: AppTab[],
	instance: AppInstanceSummary,
	presentation: AppPresentationDocument,
): OpenedAppTabs {
	const existing = tabs.find(
		(tab) => tab.kind === "oqto-app" && tab.appId === instance.app_id,
	);
	if (existing) {
		return {
			activeTabId: existing.id,
			tabs: tabs.map((tab) =>
				tab.id === existing.id && tab.kind === "oqto-app"
					? {
							...tab,
							instanceId: instance.instance_id,
							installationId: instance.installation_id,
							definitionId: instance.definition_id,
							title: instance.title.en,
							html: presentation.html,
						}
					: tab,
			),
		};
	}

	const id = `oqto-app:${instance.instance_id}`;
	return {
		activeTabId: id,
		tabs: [
			...tabs,
			{
				kind: "oqto-app",
				id,
				appId: instance.app_id,
				instanceId: instance.instance_id,
				installationId: instance.installation_id,
				definitionId: instance.definition_id,
				title: instance.title.en,
				html: presentation.html,
				pinned: false,
			},
		],
	};
}
