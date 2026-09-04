import { shouldMountInUtilityPane } from "@/features/sessions/layout-visibility";
import { describe, expect, it } from "vitest";

describe("session layout visibility", () => {
	it("unmounts an App from the utility pane while it is expanded", () => {
		expect(shouldMountInUtilityPane("app", "app", "app")).toBe(false);
	});

	it("keeps the active App in the utility pane when it is not expanded", () => {
		expect(shouldMountInUtilityPane("app", null, "app")).toBe(true);
	});

	it("does not mount an inactive utility View", () => {
		expect(shouldMountInUtilityPane("files", null, "app")).toBe(false);
	});
});
