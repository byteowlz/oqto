import { MessageGroupCard } from "@/lib/chat-rendering/CanonicalMessageRenderer";
import type { MessageGroup } from "@/lib/chat-rendering/group-messages";
import { initI18n } from "@/lib/i18n";
import { compactChatPresentation } from "@/src/oqto-ui/chat/presentation";
import { OqtoIcon } from "@/src/oqto-ui/platform/icons";
import { sharpPaths } from "@/src/oqto-ui/platform/icons/sharp-paths";
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

initI18n();
const answer = "The answer **stays complete**.";
const group = {
	role: "assistant",
	messages: [
		{
			id: "synthetic-message-a",
			role: "assistant",
			timestamp: "2026-11-19T09:41:00Z",
			parts: [
				{
					id: "thought-a",
					type: "thinking",
					text: "Checking the exact boundary.",
				},
				{
					id: "read-a",
					type: "tool_call",
					toolCallId: "call-read",
					name: "read",
					input: { path: "src/cache.ts" },
					status: "completed",
				},
				{
					id: "read-result-a",
					type: "tool_result",
					toolCallId: "call-read",
					name: "read",
					output: "unique-read-result",
					isError: false,
				},
				{
					id: "shell-a",
					type: "tool_call",
					toolCallId: "call-shell",
					name: "bash",
					input: { command: "just test" },
					status: "error",
				},
				{
					id: "shell-result-a",
					type: "tool_result",
					toolCallId: "call-shell",
					name: "bash",
					output: "unique-failed-result",
					isError: true,
				},
				{
					id: "image-a",
					type: "image",
					source: "url",
					url: "/synthetic-composition.png",
					alt: "Synthetic full composition",
				},
				{ id: "answer-a", type: "text", text: answer },
			],
		},
	],
} as unknown as MessageGroup;

beforeEach(() => {
	vi.mocked(window.localStorage.getItem).mockReturnValue(null);
});

function compact() {
	return render(
		<MessageGroupCard
			group={group}
			messageId="synthetic-message-a"
			presentation={compactChatPresentation}
		/>,
	);
}

describe("opt-in compact OqtoUI transcript", () => {
	it("counts thinking and calls in one initially closed activity, with failures visible", () => {
		const { container } = compact();
		const activity = container.querySelector(
			"details.chat-activity",
		) as HTMLDetailsElement;
		expect(activity.open).toBe(false);
		expect(activity.querySelector("summary")?.textContent).toContain("3 steps");
		expect(activity.querySelector("summary")?.textContent).toContain(
			"1 failed",
		);
		expect(container.querySelectorAll(".chat-activity")).toHaveLength(1);
		expect(screen.getByText("stays complete", { exact: false })).toBeVisible();
	});

	it("retains identified tool/error details and never mutates canonical input", () => {
		const before = JSON.stringify(group);
		const { container, rerender } = compact();
		const activity = container.querySelector(
			"details.chat-activity",
		) as HTMLDetailsElement;
		activity.open = true;
		expect(screen.getByText("Thought")).toBeVisible();
		expect(screen.getByRole("button", { name: /Running task/ })).toBeVisible();
		fireEvent.click(screen.getByRole("button", { name: /Running task/ }));
		expect(screen.getByText("unique-failed-result")).toBeVisible();
		expect(
			container.querySelector('[data-tool-status="error"]'),
		).not.toBeNull();
		expect(screen.queryByText("recovered")).toBeNull();
		rerender(
			<MessageGroupCard
				group={group}
				messageId="synthetic-message-a"
				presentation={compactChatPresentation}
				showWorkingIndicator
			/>,
		);
		expect(container.querySelector("details.chat-activity")).toBe(activity);
		expect(activity.open).toBe(true);
		expect(JSON.stringify(group)).toBe(before);
		expect(
			container
				.querySelector("[data-message-id]")
				?.getAttribute("data-message-id"),
		).toBe("synthetic-message-a");
	});

	it("keeps an inline image outside collapsed activity and forwards its original reference", () => {
		const onImageOpen = vi.fn();
		render(
			<MessageGroupCard
				group={group}
				presentation={compactChatPresentation}
				onImageOpen={onImageOpen}
			/>,
		);
		const image = screen.getByAltText("Synthetic full composition");
		expect(image.closest("details")).toBeNull();
		expect(image).toHaveClass("chat-inline-image");
		fireEvent.click(
			screen.getByRole("button", { name: /Synthetic full composition/ }),
		);
		expect(onImageOpen).toHaveBeenCalledWith(
			{ src: "/synthetic-composition.png", alt: "Synthetic full composition" },
			[
				{
					src: "/synthetic-composition.png",
					alt: "Synthetic full composition",
				},
			],
		);
	});

	it("copies the exact raw answer, with selected native Copy ink", () => {
		const writeText = vi.fn().mockResolvedValue(undefined);
		Object.defineProperty(navigator, "clipboard", {
			value: { writeText },
			configurable: true,
		});
		const { container } = compact();
		const button = screen.getByRole("button", { name: "Copy" });
		expect(button.querySelector("path")?.getAttribute("d")).toBe(
			sharpPaths.copy.paths[0],
		);
		fireEvent.click(button);
		expect(writeText).toHaveBeenCalledWith(answer);
		expect(container.querySelector(".chat-byline .sr-only")?.textContent).toBe(
			"Copy",
		);
	});

	it("keeps tool-result media visible outside activity and never duplicates it on expansion", () => {
		const media = {
			type: "image",
			alt: "Tool composition",
			media_ref: {
				session_id: "oqto-fixture",
				sha256: "a".repeat(64),
				mime_type: "image/png",
				width: 800,
				height: 480,
			},
		};
		const mediaGroup = {
			role: "assistant",
			messages: [
				{
					id: "media-message",
					role: "assistant",
					timestamp: 0,
					parts: [
						{
							type: "tool_call",
							id: "call-part",
							toolCallId: "media-call",
							name: "image-generator",
							status: "completed",
							input: {},
						},
						{
							type: "tool_result",
							id: "result-part",
							toolCallId: "media-call",
							name: "image-generator",
							output: { content: [media] },
							isError: false,
						},
					],
				},
			],
		} as unknown as MessageGroup;
		const { container } = render(
			<MessageGroupCard
				group={mediaGroup}
				presentation={compactChatPresentation}
			/>,
		);
		const picture = screen.getByAltText("Tool composition");
		expect(picture.closest("details")).toBeNull();
		expect(picture.closest("a")?.getAttribute("href")).toBe(
			`/api/chat-history/oqto-fixture/media/${"a".repeat(64)}?variant=full`,
		);
		(
			container.querySelector("details.chat-activity") as HTMLDetailsElement
		).open = true;
		fireEvent.click(
			screen.getByRole("button", { name: /image-generator.*done/ }),
		);
		expect(screen.getAllByAltText("Tool composition")).toHaveLength(1);
	});

	it("retains unknown tool and standalone-result details rather than treating them as decorative activity", () => {
		const unknownGroup = {
			role: "assistant",
			messages: [
				{
					id: "unknown-message",
					role: "assistant",
					timestamp: 0,
					parts: [
						{
							type: "tool_result",
							id: "orphan-result",
							toolCallId: "orphan-call",
							name: "future-tool",
							output: { opaque_payload: "unknown-result" },
							isError: false,
						},
					],
				},
			],
		} as unknown as MessageGroup;
		const { container } = render(
			<MessageGroupCard
				group={unknownGroup}
				presentation={compactChatPresentation}
			/>,
		);
		(
			container.querySelector("details.chat-activity") as HTMLDetailsElement
		).open = true;
		fireEvent.click(screen.getByRole("button", { name: /future-tool.*done/ }));
		expect(container.textContent).toContain("opaque_payload");
		expect(container.textContent).toContain("unknown-result");
	});

	it("preserves an explicit verbose reader preference", () => {
		vi.mocked(window.localStorage.getItem).mockImplementation((key) =>
			key === "oqto:chatVerbosity" ? "3" : null,
		);
		const { container } = compact();
		expect(container.querySelector(".chat-activity--collapsed")).toBeNull();
		expect(
			(container.querySelector("details.chat-aside") as HTMLDetailsElement)
				.open,
		).toBe(true);
	});

	it("does not change incumbent callers' presentation or activity count", () => {
		const { container } = render(<MessageGroupCard group={group} />);
		expect(container.querySelector("[data-chat-presentation]")).toBeNull();
		expect(screen.getByText("2 steps")).toBeInTheDocument();
		expect(container.querySelector(".chat-byline .sr-only")).toBeNull();
	});

	it.each(["copy", "check", "send", "attach", "file", "info"] as const)(
		"renders exact native %s vector coordinates",
		(name) => {
			const { container } = render(<OqtoIcon name={name} />);
			expect(container.querySelector("svg")?.getAttribute("viewBox")).toBe(
				sharpPaths[name].viewBox,
			);
			expect(container.querySelector("path")?.getAttribute("d")).toBe(
				sharpPaths[name].paths[0],
			);
			expect(container.querySelector("svg")?.getAttribute("aria-hidden")).toBe(
				"true",
			);
		},
	);
});
