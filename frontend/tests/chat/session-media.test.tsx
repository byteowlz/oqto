import {
	normalizeContentToParts,
	normalizeMessages,
} from "@/features/chat/hooks/message-utils";
import { MessageGroupCard } from "@/lib/chat-rendering/CanonicalMessageRenderer";
import type { MessageGroup } from "@/lib/chat-rendering/group-messages";
import { initI18n } from "@/lib/i18n";
import {
	originalSessionMediaUrl,
	sessionMediaImages,
	sessionMediaUrl,
} from "@/lib/session-media";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
initI18n();
const ref = {
	session_id: "oqto-fixture",
	sha256: "a".repeat(64),
	mime_type: "image/png",
	width: 1200,
	height: 900,
	byte_size: 1024 * 1024,
};
describe("Session media presentation", () => {
	it("uses only same-origin opaque Session/hash URLs and opens originals separately", () => {
		const src = sessionMediaUrl({
			...ref,
			host: "https://evil.invalid",
			path: "/private/file",
		});
		expect(src).toBe(
			`/api/chat-history/oqto-fixture/media/${ref.sha256}?variant=thumb`,
		);
		expect(originalSessionMediaUrl(src ?? "")).toBe(
			`/api/chat-history/oqto-fixture/media/${ref.sha256}?variant=full`,
		);
		expect(originalSessionMediaUrl(`https://evil.invalid${src}`)).toBeNull();
		expect(sessionMediaUrl({ ...ref, sha256: "../file" })).toBeNull();
		expect(
			sessionMediaUrl({ ...ref, session_id: "pending-fixture" }),
		).toBeNull();
	});
	it("deduplicates explicit refs within a tool display, without parsing text", () => {
		const image = { type: "image", media_ref: ref };
		expect(
			sessionMediaImages({
				content: [image, image, { type: "text", text: JSON.stringify(image) }],
			}),
		).toHaveLength(1);
		const parts = normalizeContentToParts([image]);
		expect(parts[0]).toMatchObject({
			type: "image",
			source: "url",
			url: sessionMediaUrl(ref),
		});
	});
	it("preserves distinct assistant message ids with identical image refs", () => {
		const content = [{ type: "image", media_ref: ref }];
		const messages = normalizeMessages([
			{ id: "message-a", role: "assistant", content },
			{ id: "message-b", role: "assistant", content },
		]);
		expect(messages.map((message) => message.id)).toEqual([
			"message-a",
			"message-b",
		]);
	});
	it("renders a thumbnail with a one-click original link, not the original inline", () => {
		const parts = normalizeContentToParts([{ type: "image", media_ref: ref }]);
		const group: MessageGroup = {
			role: "user",
			messages: [{ id: "fixture-user", role: "user", parts, timestamp: 0 }],
		};
		render(<MessageGroupCard group={group} messageId="fixture-user" />);
		const image = screen.getByRole("img") as HTMLImageElement;
		expect(image.getAttribute("src")).toBe(sessionMediaUrl(ref));
		expect(image.closest("a")?.getAttribute("href")).toBe(
			originalSessionMediaUrl(sessionMediaUrl(ref) ?? ""),
		);
	});
});
