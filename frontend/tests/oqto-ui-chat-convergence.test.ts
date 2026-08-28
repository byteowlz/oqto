import { describe, expect, it, vi } from "vitest";
import {
	dropActiveScriptedSocket,
	scriptedChat,
	scriptedOqtoUiPlatform,
	scriptedStore,
} from "../src/oqto-ui/dev/scripted-platform";

const SESSION = "frontend-rebuild";

describe("writable chat convergence (oqto-a9j4.4 proof)", () => {
	it("converges to store truth when the transport dies mid-turn", async () => {
		const updates: string[] = [];
		const sawDraft = vi.fn();
		const detach = scriptedChat.bind(SESSION, (update) => {
			updates.push(update.kind);
			if (update.kind === "draft") sawDraft();
		});

		scriptedChat.send(SESSION, "ping during flaky turn", "steer");

		// The turn starts streaming through the real transport+projection.
		await vi.waitFor(() => expect(sawDraft).toHaveBeenCalled(), {
			timeout: 2000,
		});

		// Kill the socket mid-turn, before the scripted reply completes.
		dropActiveScriptedSocket();

		// Reconnect happens via backoff; the engine reports the loss
		// (ended/resync) instead of owning state, and the scripted server
		// has already persisted the user message as durable truth.
		await vi.waitFor(
			() => {
				expect(updates.some((k) => k === "ended" || k === "resync")).toBe(true);
			},
			{ timeout: 5000 },
		);

		const persisted = scriptedStore.get(SESSION) ?? [];
		expect(
			persisted.some(
				(message) =>
					message.author === "user" &&
					message.content === "ping during flaky turn",
			),
		).toBe(true);

		// The store never received the interrupted assistant turn.
		const lastAssistant = persisted
			.filter((message) => message.author === "assistant")
			.at(-1);
		expect(lastAssistant?.content ?? "").not.toContain(
			"scripted engine streamed",
		);

		// A page refetch (what the UI does on turn end/resync) returns the
		// authoritative state: the user message is in, no phantom turn.
		const page = await scriptedOqtoUiPlatform.loadMessages(SESSION);
		expect(
			page.messages.some(
				(message) =>
					message.author === "user" &&
					message.content === "ping during flaky turn",
			),
		).toBe(true);

		detach();
	});
});
