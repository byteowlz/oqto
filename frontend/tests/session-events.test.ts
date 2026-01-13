import { describe, expect, it } from "vitest";

import {
	normalizePermissionEvent,
	parseSessionErrorEvent,
} from "@/lib/session-events";

describe("session-events", () => {
	describe("normalizePermissionEvent", () => {
		it("handles snake_case permission fields", () => {
			const permission = normalizePermissionEvent({
				properties: {
					permission_id: "perm-1",
					permission_type: "edit",
					title: "Edit file",
					pattern: "README.md",
				},
			});
			expect(permission).toEqual(
				expect.objectContaining({
					id: "perm-1",
					type: "edit",
					title: "Edit file",
					pattern: "README.md",
				}),
			);
		});

		it("accepts camelCase permission fields", () => {
			const permission = normalizePermissionEvent({
				properties: {
					permissionId: "perm-2",
					permissionType: "bash",
					title: "Run bash",
				},
			});
			expect(permission).toEqual(
				expect.objectContaining({
					id: "perm-2",
					type: "bash",
					title: "Run bash",
				}),
			);
		});
	});

	describe("parseSessionErrorEvent", () => {
		it("handles flat error fields", () => {
			const errorInfo = parseSessionErrorEvent({
				error_type: "BadRequest",
				message: "Nope",
			});
			expect(errorInfo).toEqual({
				name: "BadRequest",
				message: "Nope",
			});
		});

		it("handles nested error payloads", () => {
			const errorInfo = parseSessionErrorEvent({
				error: {
					name: "UnknownError",
					data: { message: "Boom" },
				},
			});
			expect(errorInfo).toEqual({
				name: "UnknownError",
				message: "Boom",
			});
		});
	});
});
