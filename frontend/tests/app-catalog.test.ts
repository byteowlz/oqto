import { buildAppCatalogEntries } from "@/features/sessions/components/AppView";
import type { AppCandidateSummary } from "@/src/generated/AppCandidateSummary";
import type { AppInstanceSummary } from "@/src/generated/AppInstanceSummary";
import { describe, expect, it } from "vitest";

function candidate(appId: string): AppCandidateSummary {
	return {
		app_id: appId,
		title: { en: "Comfy Studio" },
		version: "0.1.0",
		presentations: [],
		requested_capabilities: [],
		state: "publishable",
	};
}

function instance(appId: string): AppInstanceSummary {
	return {
		instance_id: `instance-${appId}`,
		definition_id: `definition-${appId}`,
		installation_id: `installation-${appId}`,
		app_id: appId,
		title: { en: "Comfy Studio" },
		version: "0.1.0",
		content_digest: "a".repeat(64),
		installation_owner_kind: "account",
		binding_kind: "work_directory",
		status: "active",
		presentations: [],
	};
}

describe("App catalog entries", () => {
	it("shows discovered and installed representations as one App", () => {
		const entries = buildAppCatalogEntries(
			[candidate("comfy-studio")],
			[instance("comfy-studio")],
		);

		expect(entries).toHaveLength(1);
		expect(entries[0]?.candidate?.app_id).toBe("comfy-studio");
		expect(entries[0]?.instance?.app_id).toBe("comfy-studio");
	});

	it("retains installed Apps whose source package is unavailable", () => {
		const entries = buildAppCatalogEntries([], [instance("archived-app")]);
		expect(entries.map((entry) => entry.appId)).toEqual(["archived-app"]);
	});
});
