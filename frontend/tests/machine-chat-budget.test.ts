import { CACHE_LIMITS, selectEvictions } from "@/lib/machine-chat-cache";
import { describe, expect, it } from "vitest";

const limits = { maxEntryBytes: 1000, maxTotalBytes: 300, maxEntries: 3 };

describe("machine chat cache budget", () => {
	it("keeps everything while inside the budget", () => {
		expect(
			selectEvictions(
				[
					{ key: "a", bytes: 100, lastReadAt: 1 },
					{ key: "b", bytes: 100, lastReadAt: 2 },
				],
				limits,
			),
		).toEqual([]);
	});

	it("evicts least recently read first when over the byte budget", () => {
		const evicted = selectEvictions(
			[
				{ key: "recent", bytes: 150, lastReadAt: 300 },
				{ key: "oldest", bytes: 150, lastReadAt: 100 },
				{ key: "middle", bytes: 150, lastReadAt: 200 },
			],
			limits,
		);
		// 450 bytes against a 300 budget: drop the oldest until it fits.
		expect(evicted).toEqual(["oldest"]);
	});

	it("evicts on entry count even when the conversations are small", () => {
		const evicted = selectEvictions(
			[
				{ key: "a", bytes: 1, lastReadAt: 1 },
				{ key: "b", bytes: 1, lastReadAt: 2 },
				{ key: "c", bytes: 1, lastReadAt: 3 },
				{ key: "d", bytes: 1, lastReadAt: 4 },
				{ key: "e", bytes: 1, lastReadAt: 5 },
			],
			limits,
		);
		expect(evicted).toEqual(["a", "b"]);
	});

	it("never evicts the conversation just read", () => {
		const entries = [
			{ key: "just-read", bytes: 290, lastReadAt: 999 },
			{ key: "older-1", bytes: 290, lastReadAt: 1 },
			{ key: "older-2", bytes: 290, lastReadAt: 2 },
		];
		expect(selectEvictions(entries, limits)).not.toContain("just-read");
	});

	it("ships a budget far below what a machine's full history would need", () => {
		// 2964 chats at ~150KB of newest page each would be hundreds of MB; the
		// cache is a front for the machine, not a mirror of it.
		expect(CACHE_LIMITS.maxEntries).toBeLessThanOrEqual(100);
		expect(CACHE_LIMITS.maxTotalBytes).toBeLessThanOrEqual(128 * 1024 * 1024);
	});
});
