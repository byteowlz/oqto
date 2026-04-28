import {
	beginMessageSync,
	bindIdentity,
	completeMessageSync,
	createInitialChatStateMachine,
	deriveUiFlags,
	resetIdentity,
	transitionTransport,
	transitionTurn,
} from "@/features/chat/hooks/chat-state-machine";
import { describe, expect, it } from "vitest";

describe("chat state machine", () => {
	it("rejects non-idle turn transitions while unbound", () => {
		const machine = createInitialChatStateMachine("oqto-abc");
		const next = transitionTurn(machine, { kind: "sending" });
		expect(next.turn.kind).toBe("idle");
	});

	it("binds identity and allows canonical turn lifecycle", () => {
		const machine = createInitialChatStateMachine("oqto-abc");
		const bound = bindIdentity(machine, {
			runnerId: "runner-session-1",
			hstryId: "hstry-1",
			piId: "pi-1",
		});
		expect(bound.identity.kind).toBe("bound");
		if (bound.identity.kind !== "bound") return;
		expect(bound.identity.runnerId).toBe("runner-session-1");

		const sending = transitionTurn(bound, {
			kind: "sending",
			commandId: "cmd-1",
			clientMessageId: "msg-1",
		});
		expect(sending.turn.kind).toBe("sending");

		const streaming = transitionTurn(sending, {
			kind: "streaming",
			turnId: "t1",
		});
		expect(streaming.turn.kind).toBe("streaming");
		expect(deriveUiFlags(streaming.turn)).toEqual({
			isStreaming: true,
			isAwaitingResponse: false,
		});

		const syncing = transitionTurn(streaming, { kind: "syncing" });
		expect(syncing.turn.kind).toBe("syncing");

		const idle = transitionTurn(syncing, { kind: "idle" });
		expect(idle.turn.kind).toBe("idle");
		expect(deriveUiFlags(idle.turn)).toEqual({
			isStreaming: false,
			isAwaitingResponse: false,
		});
	});

	it("allows idle -> streaming for runner-initiated turns", () => {
		const bound = bindIdentity(createInitialChatStateMachine("oqto-abc"), {
			runnerId: "runner-session-1",
		});
		const streaming = transitionTurn(bound, {
			kind: "streaming",
			turnId: "remote-turn-1",
		});
		expect(streaming.turn.kind).toBe("streaming");
		expect(deriveUiFlags(streaming.turn)).toEqual({
			isStreaming: true,
			isAwaitingResponse: false,
		});
	});

	it("ignores stale transport epochs", () => {
		const machine = createInitialChatStateMachine("oqto-abc");
		const connected = transitionTransport(machine, "connected", 2);
		expect(connected.transport.kind).toBe("connected");
		expect(connected.transport.epoch).toBe(2);

		const stale = transitionTransport(connected, "reconnecting", 1);
		expect(stale.transport.kind).toBe("connected");
		expect(stale.transport.epoch).toBe(2);
	});

	it("all server data is authoritative (no merge mode selection)", () => {
		// With the runner message buffer as single authority, all server
		// responses are treated as authoritative. No source-dependent
		// merge mode selection needed.
		const machine = bindIdentity(createInitialChatStateMachine("oqto-abc"), {
			runnerId: "runner-session-1",
		});
		const sending = transitionTurn(machine, {
			kind: "sending",
			commandId: "cmd-1",
			clientMessageId: "msg-1",
		});
		const streaming = transitionTurn(sending, {
			kind: "streaming",
			turnId: "t1",
		});
		const syncing = transitionTurn(streaming, { kind: "syncing" });
		expect(syncing.turn.kind).toBe("syncing");
		// Syncing -> idle is always valid
		const idle = transitionTurn(syncing, { kind: "idle" });
		expect(idle.turn.kind).toBe("idle");
	});

	it("tracks sync revision lifecycle", () => {
		const machine = createInitialChatStateMachine("oqto-abc");
		const syncing = beginMessageSync(machine);
		expect(syncing.sync.phase).toBe("syncing");
		const done = completeMessageSync(syncing);
		expect(done.sync.phase).toBe("idle");
		expect(done.sync.revision).toBe(1);
	});

	it("resets identity to unbound and turn to idle", () => {
		const machine = bindIdentity(createInitialChatStateMachine("oqto-abc"), {
			runnerId: "runner-session-1",
		});
		const sending = transitionTurn(machine, {
			kind: "sending",
			commandId: "cmd-1",
			clientMessageId: "msg-1",
		});
		const reset = resetIdentity(sending, "oqto-new");
		expect(reset.identity.kind).toBe("unbound");
		expect(reset.turn.kind).toBe("idle");
	});
});
