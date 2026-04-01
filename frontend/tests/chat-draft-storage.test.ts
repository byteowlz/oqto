import {
	buildLegacyDraftStorageKey,
	buildSessionDraftStorageKey,
} from "@/features/chat/hooks/draft-storage";
import { describe, expect, it } from "vitest";

describe("chat draft storage keys", () => {
	it("scopes draft keys per session id", () => {
		const prefix = "oqto:workspacePi:my_ws";

		expect(buildSessionDraftStorageKey(prefix, "session-a")).toBe(
			"oqto:workspacePi:my_ws:session:session-a:draft",
		);
		expect(buildSessionDraftStorageKey(prefix, "session-b")).toBe(
			"oqto:workspacePi:my_ws:session:session-b:draft",
		);
	});

	it("sanitizes session ids so storage keys are safe and deterministic", () => {
		const prefix = "oqto:workspacePi:my_ws";
		expect(buildSessionDraftStorageKey(prefix, "session/one two")).toBe(
			"oqto:workspacePi:my_ws:session:session_one_two:draft",
		);
	});

	it("uses fallback scope when there is no active session", () => {
		const prefix = "oqto:workspacePi:my_ws";
		expect(buildSessionDraftStorageKey(prefix, null)).toBe(
			"oqto:workspacePi:my_ws:session:__no_session__:draft",
		);
	});

	it("keeps legacy workspace draft key distinct from session-scoped keys", () => {
		const prefix = "oqto:workspacePi:my_ws";
		expect(buildLegacyDraftStorageKey(prefix)).toBe(
			"oqto:workspacePi:my_ws:draft",
		);
		expect(buildSessionDraftStorageKey(prefix, "session-a")).not.toBe(
			buildLegacyDraftStorageKey(prefix),
		);
	});
});
