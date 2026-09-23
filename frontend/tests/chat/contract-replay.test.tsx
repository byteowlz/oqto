import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { appendDeltaPart } from "@/features/chat/hooks/canonical-event-reducer";
import type { DisplayMessage } from "@/features/chat/hooks/types";
import type { MessageGroup } from "@/lib/chat-rendering/group-messages";
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MessageGroupCard } from "../../lib/chat-rendering/CanonicalMessageRenderer";
import { initI18n } from "../../lib/i18n";

/**
 * Replays the shared chat corpus (contracts/chat) against this client.
 *
 * The desktop client replays the same files against its own timeline. Neither
 * side reads the other's code: the fixtures are the contract, and a fixture
 * this client fails is behaviour it has not implemented yet.
 */

initI18n();

const CORPUS = resolve(process.cwd(), "../contracts/chat/fixtures");

type Block =
	| { type: "text"; text: string }
	| { type: "thinking"; text: string }
	| { type: "tool_call"; name: string; arguments?: Record<string, unknown> };

type Fixture =
	| {
			format: "oqto-chat-stream";
			name: string;
			why: string;
			events: Array<Record<string, unknown>>;
			expect: { blocks: Block[] };
	  }
	| {
			format: "oqto-chat-presentation";
			name: string;
			why: string;
			level: 1 | 2 | 3;
			blocks: Block[];
			expect: { answer: string[]; steps?: string[]; hidden?: string[] };
	  }
	| {
			format: "oqto-chat-user-message";
			name: string;
			why: string;
			text: string;
			expect: { text: string; attachments?: string[] };
	  };

const fixtures: Fixture[] = readdirSync(CORPUS)
	.filter((file) => file.endsWith(".json"))
	.sort()
	.map((file) => JSON.parse(readFileSync(resolve(CORPUS, file), "utf8")));

const normalize = (text: string | null | undefined) =>
	(text ?? "").replace(/\s+/g, " ").trim();

let stored: Record<string, string> = {};
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
afterEach(cleanup);

/**
 * pi AgentEvent → this client's parts, the way the runner's translator and the
 * stream reducer do it between them.
 */
function replayStream(events: Array<Record<string, unknown>>): DisplayMessage {
	const message = {
		id: "m",
		role: "assistant",
		parts: [],
		timestamp: 0,
	} as unknown as DisplayMessage;
	let counter = 0;
	const nextPartId = () => `p${++counter}`;
	for (const event of events) {
		if (event.type !== "message_update") continue;
		const inner = event.assistantMessageEvent as Record<string, unknown>;
		const contentIndex = inner.contentIndex as number;
		if (inner.type === "text_delta" || inner.type === "thinking_delta") {
			appendDeltaPart({
				message,
				delta: inner.delta as string,
				partType: inner.type === "text_delta" ? "text" : "thinking",
				contentIndex,
				nextPartId,
			});
		} else if (inner.type === "toolcall_end") {
			const call = inner.toolCall as Record<string, unknown>;
			message.parts.push({
				type: "tool_call",
				id: nextPartId(),
				toolCallId: call.id as string,
				name: call.name as string,
				input: call.arguments,
				status: "success",
			} as unknown as DisplayMessage["parts"][number]);
		}
	}
	return message;
}

function toBlocks(message: DisplayMessage): Block[] {
	return message.parts.map((part) => {
		if (part.type === "tool_call") {
			return { type: "tool_call", name: (part as { name: string }).name };
		}
		return {
			type: part.type as "text" | "thinking",
			text: (part as { text: string }).text,
		};
	});
}

function groupOf(role: "user" | "assistant", blocks: Block[]): MessageGroup {
	return {
		role,
		messages: [
			{
				id: `${role}-1`,
				role,
				timestamp: "2026-09-23T09:00:00.000Z",
				parts: blocks.map((block, index) =>
					block.type === "tool_call"
						? {
								type: "tool_call",
								id: `call-${index}`,
								toolCallId: `call-${index}`,
								name: block.name,
								input: block.arguments ?? {},
								status: "completed",
							}
						: { type: block.type, id: `part-${index}`, text: block.text },
				),
			},
		],
	} as unknown as MessageGroup;
}

describe("chat contract corpus", () => {
	it("is not empty", () => {
		expect(fixtures.length).toBeGreaterThan(0);
	});

	for (const fixture of fixtures) {
		it(`${fixture.format} · ${fixture.name}`, () => {
			if (fixture.format === "oqto-chat-stream") {
				const blocks = toBlocks(replayStream(fixture.events));
				expect(blocks, fixture.why).toEqual(
					fixture.expect.blocks.map((block) =>
						block.type === "tool_call"
							? { type: "tool_call", name: block.name }
							: block,
					),
				);
				return;
			}

			if (fixture.format === "oqto-chat-presentation") {
				stored["oqto:chatVerbosity"] = String(fixture.level);
				const { container } = render(
					<MessageGroupCard
						group={groupOf("assistant", fixture.blocks)}
						messageId="assistant-1"
					/>,
				);
				const answer = [
					...container.querySelectorAll(".canonical-message-prose"),
				].map((node) => normalize(node.textContent));
				expect(answer, fixture.why).toEqual(
					fixture.expect.answer.map(normalize),
				);
				if (fixture.expect.steps) {
					const steps = [
						...container.querySelectorAll(".chat-activity ol li"),
					].map((node) => normalize(node.textContent));
					expect(steps, fixture.why).toEqual(
						fixture.expect.steps.map(normalize),
					);
				}
				for (const text of fixture.expect.hidden ?? []) {
					expect(normalize(container.textContent), fixture.why).not.toContain(
						normalize(text),
					);
				}
				return;
			}

			const { container } = render(
				<MessageGroupCard
					group={groupOf("user", [{ type: "text", text: fixture.text }])}
					messageId="user-1"
					workspacePath="/w"
				/>,
			);
			const shown = [...container.querySelectorAll(".canonical-message-prose")]
				.map((node) => normalize(node.textContent))
				.join(" ");
			expect(shown, fixture.why).toBe(normalize(fixture.expect.text));
			if (fixture.expect.attachments) {
				const attachments = [
					...container.querySelectorAll(".chat-chip[title]"),
				].map((node) => node.getAttribute("title"));
				expect(attachments, fixture.why).toEqual(fixture.expect.attachments);
			}
		});
	}
});
