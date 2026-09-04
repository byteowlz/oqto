import { openOqtoAppTab } from "@/features/sessions/app-tabs";
import type { AppTab } from "@/features/sessions/components/AppView";
import type { AppInstanceSummary } from "@/src/generated/AppInstanceSummary";
import { describe, expect, it } from "vitest";

function instance(id: string): AppInstanceSummary {
	return {
		instance_id: id,
		definition_id: `definition-${id}`,
		installation_id: `installation-${id}`,
		app_id: "comfy-studio",
		title: { en: "Comfy Studio" },
		version: "0.1.0",
		content_digest: "a".repeat(64),
		installation_owner_kind: "account",
		binding_kind: "work_directory",
		status: "active",
		presentations: [],
	};
}

describe("Oqto App tabs", () => {
	it("replaces a superseded Instance without opening a duplicate tab", () => {
		const original: AppTab = {
			kind: "oqto-app",
			id: "oqto-app:old",
			appId: "comfy-studio",
			instanceId: "old",
			installationId: "installation-old",
			definitionId: "definition-old",
			title: "Comfy Studio",
			html: "old presentation",
			pinned: false,
		};

		const opened = openOqtoAppTab([original], instance("new"), {
			instance_id: "new",
			presentation_id: "main",
			definition_id: "definition-new",
			content_digest: "b".repeat(64),
			html: "new presentation",
		});

		expect(opened.tabs).toHaveLength(1);
		expect(opened.activeTabId).toBe(original.id);
		expect(opened.tabs[0]).toMatchObject({
			id: original.id,
			instanceId: "new",
			definitionId: "definition-new",
			html: "new presentation",
		});
	});
});
