import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { MessageGroup } from "@/lib/chat-rendering/group-messages";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MessageGroupCard } from "../../lib/chat-rendering/CanonicalMessageRenderer";
import { initI18n } from "../../lib/i18n";

initI18n();

/**
 * The renderer reads the detail level out of the host store, so the suite
 * needs a localStorage that remembers rather than the shared mock.
 */
let stored: { [key: string]: string } = {};

beforeEach(() => {
	stored = {};
	vi.mocked(window.localStorage.getItem).mockImplementation(
		(key: string) => stored[key] ?? null,
	);
	vi.mocked(window.localStorage.setItem).mockImplementation(
		(key: string, value: string) => {
			stored[key] = value;
		},
	);
});

function group(
	role: "user" | "assistant",
	parts: MessageGroup["messages"][number]["parts"],
): MessageGroup {
	return {
		role,
		messages: [
			{
				id: `${role}-1`,
				role,
				parts,
				timestamp: "2026-09-11T09:41:00.000Z",
			},
		],
	} as unknown as MessageGroup;
}

const ANSWER = group("assistant", [
	{ type: "text", text: "Die Verteilung hängt an **F1**." },
	{
		type: "tool_call",
		toolCallId: "call-1",
		name: "read",
		input: { filePath: "knx/lights.yaml" },
		status: "completed",
	},
	{ type: "thinking", text: "Mapping the fuse table onto the actor groups." },
	{ type: "text", text: "Danach reicht ein Reload." },
] as unknown as MessageGroup["messages"][number]["parts"]);

describe("chat message rendering", () => {
	it("gives the user turn a bubble and the assistant turn plain prose", () => {
		const { container, rerender } = render(
			<MessageGroupCard
				group={group("user", [
					{ type: "text", text: "Zeig mir die Verteilung." },
				] as unknown as MessageGroup["messages"][number]["parts"])}
				messageId="user-1"
			/>,
		);
		expect(container.querySelector(".chat-turn--user")).not.toBeNull();
		expect(container.querySelector(".chat-bubble")).not.toBeNull();

		rerender(<MessageGroupCard group={ANSWER} messageId="assistant-1" />);
		expect(container.querySelector(".chat-turn--user")).toBeNull();
		expect(container.querySelector(".chat-bubble")).toBeNull();
	});

	it("sets the answer as prose rather than in the monospace UI face", () => {
		const { container } = render(
			<MessageGroupCard group={ANSWER} messageId="assistant-1" />,
		);
		const prose = container.querySelector(".canonical-message-prose");
		expect(prose?.classList.contains("chat-prose")).toBe(true);
		// The scale belongs to the stylesheet, so the renderer must not pin it.
		expect(prose?.className).not.toMatch(/\btext-(xs|sm|base|lg)\b/);
	});

	it("puts copy in the byline without baking its visibility into markup", () => {
		const { container } = render(
			<MessageGroupCard group={ANSWER} messageId="assistant-1" />,
		);
		const byline = container.querySelector(".chat-byline");
		const copy = byline?.querySelector("button.chat-action");
		expect(copy).not.toBeNull();
		// Whether it waits for a hover is a preference and a media query, so
		// the markup must not hard-code either answer.
		expect(byline?.className).not.toMatch(/opacity-0|group-hover/);
		expect(copy?.className).not.toMatch(/opacity-0|group-hover/);
	});

	it("never hides the actions where there is no pointer to hover with", () => {
		const stylesheet = readFileSync(
			resolve(process.cwd(), "lib/chat-rendering/chat-typography.css"),
			"utf8",
		);
		const hidden = stylesheet.slice(
			stylesheet.indexOf(".chat-actions {"),
			stylesheet.indexOf(".chat-action {"),
		);
		// The only rule that hides them sits behind a hover-capable pointer,
		// and the reader can switch it off entirely.
		expect(hidden).toMatch(
			/@media \(hover: hover\) and \(pointer: fine\)[\s\S]*opacity: 0;/,
		);
		expect(hidden).toMatch(/\[data-chat-actions="always"\]/);
		expect(hidden.indexOf("opacity: 0;")).toBeGreaterThan(
			hidden.indexOf("@media (hover: hover)"),
		);
	});

	it("gives the user turn the full column on a phone", () => {
		const stylesheet = readFileSync(
			resolve(process.cwd(), "lib/chat-rendering/chat-typography.css"),
			"utf8",
		);
		const phone = stylesheet.slice(
			stylesheet.indexOf("@media (max-width: 40rem)"),
		);

		// On a narrow screen the question and the answer below it must share one
		// measure, so the bubble drops its width cap and stops hugging the edge.
		expect(phone).toMatch(/\.chat-bubble \{\s*max-inline-size: 100%;/);
		expect(phone).toMatch(/\.chat-turn--user \{\s*align-items: stretch;/);

		// The cap still governs a wide screen; this is a width rule, not a
		// different shape for the turn.
		const wide = stylesheet.slice(
			0,
			stylesheet.indexOf("@media (max-width: 40rem)"),
		);
		expect(wide).toMatch(/max-inline-size: min\(80%, 62ch\);/);
	});

	it("folds tool calls into one activity line at the lowest detail level", () => {
		stored["oqto:chatVerbosity"] = "1";
		const { container } = render(
			<MessageGroupCard group={ANSWER} messageId="assistant-1" />,
		);
		expect(container.querySelectorAll(".chat-activity")).toHaveLength(1);
		// The level governs the work, never the answer.
		expect(screen.getByText(/Danach reicht ein Reload/)).toBeInTheDocument();
		expect(
			screen.queryByText(/Mapping the fuse table/),
		).not.toBeInTheDocument();
	});

	it("shows every step at the highest detail level", () => {
		stored["oqto:chatVerbosity"] = "3";
		const { container } = render(
			<MessageGroupCard group={ANSWER} messageId="assistant-1" />,
		);
		expect(container.querySelector(".chat-activity")).toBeNull();
		expect(screen.getByText(/Danach reicht ein Reload/)).toBeInTheDocument();
	});
});

describe("announcements and the calls they introduce", () => {
	const ANNOUNCED = group("assistant", [
		{ type: "text", text: "Let me check how the frame reads files:" },
		{
			type: "tool_call",
			toolCallId: "call-a",
			name: "read",
			input: { filePath: "frame.ts" },
			status: "completed",
		},
		{ type: "text", text: "Scaffold created. Let me review it:" },
		{
			type: "tool_call",
			toolCallId: "call-b",
			name: "read",
			input: { filePath: "manifest.toml" },
			status: "completed",
		},
		{ type: "text", text: "Fertig — die App ist gebaut und getestet." },
	] as unknown as MessageGroup["messages"][number]["parts"]);

	it("does not strand announcements when the calls leave the flow", () => {
		stored["oqto:chatVerbosity"] = "1";
		render(<MessageGroupCard group={ANNOUNCED} messageId="assistant-2" />);
		// The answer stays.
		expect(screen.getByText(/die App ist gebaut/)).toBeInTheDocument();
		// The sentences that only introduced a call travel with it.
		expect(
			screen.queryByText(/Let me check how the frame reads files:/),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText(/Scaffold created\. Let me review it:/),
		).not.toBeInTheDocument();
	});

	it("labels each step with what the agent said it was doing", () => {
		stored["oqto:chatVerbosity"] = "1";
		const { container } = render(
			<MessageGroupCard group={ANNOUNCED} messageId="assistant-2" />,
		);
		const steps = [...container.querySelectorAll(".chat-activity ol li")].map(
			(item) => item.textContent,
		);
		expect(steps).toEqual([
			"Let me check how the frame reads files",
			"Scaffold created. Let me review it",
		]);
	});

	it("keeps a paragraph that merely precedes a call in the answer", () => {
		stored["oqto:chatVerbosity"] = "1";
		render(
			<MessageGroupCard
				group={group("assistant", [
					{
						type: "text",
						text: "Die Verteilung hängt an F1 und wird über den Bus geschaltet. Das ist die Kernlogik, und sie gilt für jede Gruppe im Haus ohne Ausnahme.",
					},
					{
						type: "tool_call",
						toolCallId: "call-c",
						name: "read",
						input: { filePath: "x.ts" },
						status: "completed",
					},
				] as unknown as MessageGroup["messages"][number]["parts"])}
				messageId="assistant-3"
			/>,
		);
		expect(
			screen.getAllByText(/Die Verteilung hängt an F1/).length,
		).toBeGreaterThan(0);
	});
});
