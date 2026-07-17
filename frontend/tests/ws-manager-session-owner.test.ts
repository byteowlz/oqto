import { beforeEach, describe, expect, it, vi } from "vitest";

import { destroyWsManager, getWsManager } from "@/lib/ws-manager";
import type { AgentWsEvent, WsEvent } from "@/lib/ws-mux-types";

type TestableManager = {
	handleEvent: (event: WsEvent) => void;
};

const textDelta = (sessionId: string): AgentWsEvent =>
	({
		channel: "agent",
		session_id: sessionId,
		event: "stream.text_delta",
		message_id: "message-1",
		content_index: 0,
		delta: "word",
	}) as AgentWsEvent;

describe("WsConnectionManager session event ownership", () => {
	beforeEach(() => {
		destroyWsManager();
	});

	it("delivers additive stream events only to the latest owner", () => {
		const manager = getWsManager();
		const firstOwner = vi.fn();
		const latestOwner = vi.fn();

		manager.subscribeAgentSession("session-1", firstOwner, undefined, {
			create: false,
		});
		manager.subscribeAgentSession("session-1", latestOwner, undefined, {
			create: false,
		});

		(manager as unknown as TestableManager).handleEvent(
			textDelta("session-1") as WsEvent,
		);

		expect(firstOwner).not.toHaveBeenCalled();
		expect(latestOwner).toHaveBeenCalledTimes(1);
	});

	it("does not let a stale unsubscribe remove the replacement owner", () => {
		const manager = getWsManager();
		const firstOwner = vi.fn();
		const latestOwner = vi.fn();
		const unsubscribeFirst = manager.subscribeAgentSession(
			"session-1",
			firstOwner,
			undefined,
			{ create: false },
		);
		manager.subscribeAgentSession("session-1", latestOwner, undefined, {
			create: false,
		});

		unsubscribeFirst();
		(manager as unknown as TestableManager).handleEvent(
			textDelta("session-1") as WsEvent,
		);

		expect(firstOwner).not.toHaveBeenCalled();
		expect(latestOwner).toHaveBeenCalledTimes(1);
	});
});
