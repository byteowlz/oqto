import { preferStableSessionTitle } from "@/lib/session-utils";
import { describe, expect, it } from "vitest";

describe("preferStableSessionTitle", () => {
	const NEW = "New Session";
	const ACTIVE = "Active Session";

	it("keeps an existing generated title when incoming fallback is New Session", () => {
		const next = preferStableSessionTitle(
			"Strategy Workshop PDF Analysis",
			NEW,
			NEW,
			ACTIVE,
		);
		expect(next).toBe("Strategy Workshop PDF Analysis");
	});

	it("keeps an existing generated title when incoming fallback is localized active session", () => {
		const next = preferStableSessionTitle(
			"Strategy Workshop PDF Analysis",
			"Aktive Sitzung",
			"Neue Sitzung",
			"Aktive Sitzung",
		);
		expect(next).toBe("Strategy Workshop PDF Analysis");
	});

	it("accepts incoming generated title over previous fallback", () => {
		const next = preferStableSessionTitle(
			NEW,
			"Strategy Workshop PDF Analysis",
			NEW,
			ACTIVE,
		);
		expect(next).toBe("Strategy Workshop PDF Analysis");
	});
});
