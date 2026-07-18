import {
	INITIAL_VISIBLE_MESSAGE_COUNT,
	LOAD_MORE_MESSAGE_COUNT,
	nextVisibleMessageCount,
} from "@/features/chat/components/ChatView";
import { describe, expect, it } from "vitest";

describe("chat visible message window", () => {
	it("keeps initial history rendering bounded for very large chats", () => {
		expect(INITIAL_VISIBLE_MESSAGE_COUNT).toBe(30);
		// Loading a 5,000-message payload must not implicitly widen the window.
		const visibleAfterHistoryHydration = INITIAL_VISIBLE_MESSAGE_COUNT;
		expect(visibleAfterHistoryHydration).toBeLessThan(5_000);
	});

	it("loads older messages in bounded increments", () => {
		expect(nextVisibleMessageCount(30, 5_000)).toBe(
			30 + LOAD_MORE_MESSAGE_COUNT,
		);
		expect(nextVisibleMessageCount(4_990, 5_000)).toBe(5_000);
	});
});
