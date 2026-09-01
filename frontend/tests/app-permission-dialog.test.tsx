import { AppPermissionDialog } from "@/features/sessions/components/AppPermissionDialog";
import type { AppPermissionRequest } from "@/src/generated/AppPermissionRequest";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { initI18n } from "../lib/i18n";

initI18n();

const request: AppPermissionRequest = {
	instance_id: "appinstance_1",
	definition_id: "appdef_1",
	content_digest: "a".repeat(64),
	app_id: "comfy-studio",
	title: { en: "Comfy Studio" },
	version: "0.1.0",
	capabilities: [
		{
			capability: "files",
			resources: [
				{ role: "outputs", path: "outputs", access: "read", watch: true },
				{
					role: "requests",
					path: "oqto-apps-state/comfy-studio/requests",
					access: "read_write",
					watch: false,
				},
			],
		},
		{
			capability: "operations",
			operations: [
				{
					id: "comfy.generate.submit",
					summary: "Submit one image generation",
				},
			],
		},
		{ capability: "theme" },
		{ capability: "kv" },
	],
};

describe("AppPermissionDialog", () => {
	it("explains the grant in plain language before technical details", () => {
		render(
			<AppPermissionDialog
				request={request}
				busy={false}
				error={null}
				onAllow={vi.fn()}
				onNotNow={vi.fn()}
				onClose={vi.fn()}
			/>,
		);

		expect(screen.getByText("Let Comfy Studio do these things?")).toBeTruthy();
		expect(screen.getByText("Files used by this App")).toBeTruthy();
		expect(screen.getByText("ComfyUI actions")).toBeTruthy();
		expect(
			screen.getByText(/This App is separate from your agent/),
		).toBeTruthy();
		expect(screen.queryByText("comfy.generate.submit")).toBeNull();
		expect(
			screen.queryByText("oqto-apps-state/comfy-studio/requests"),
		).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: "Show exact access" }));
		expect(screen.getByText("comfy.generate.submit")).toBeTruthy();
		expect(
			screen.getByText("oqto-apps-state/comfy-studio/requests"),
		).toBeTruthy();
	});

	it("uses direct allow and not-now actions rather than consent prose", () => {
		const onAllow = vi.fn();
		const onNotNow = vi.fn();
		render(
			<AppPermissionDialog
				request={request}
				busy={false}
				error={null}
				onAllow={onAllow}
				onNotNow={onNotNow}
				onClose={vi.fn()}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Allow" }));
		fireEvent.click(screen.getByRole("button", { name: "Not now" }));
		expect(onAllow).toHaveBeenCalledOnce();
		expect(onNotNow).toHaveBeenCalledOnce();
		expect(screen.queryByText(/agree|terms|consent/i)).toBeNull();
	});
});
