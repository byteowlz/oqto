import {
	beginMessageSync,
	bindIdentity,
	completeMessageSync,
	createInitialChatStateMachine,
	deriveUiFlags,
	resetIdentity,
	selectMessageMergeMode,
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

		const reconciling = transitionTurn(streaming, {
			kind: "reconciling",
			reason: "idle",
		});
		expect(reconciling.turn.kind).toBe("reconciling");

		const idle = transitionTurn(reconciling, { kind: "idle" });
		expect(idle.turn.kind).toBe("idle");
		expect(deriveUiFlags(idle.turn)).toEqual({
			isStreaming: false,
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

	it("uses partial merge for ws_get_messages during streaming, authoritative otherwise", () => {
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
		expect(selectMessageMergeMode(streaming, "ws_get_messages")).toBe(
			"partial",
		);
		expect(selectMessageMergeMode(streaming, "history")).toBe("partial");
		const reconciling = transitionTurn(streaming, {
			kind: "reconciling",
			reason: "resync",
		});
		expect(selectMessageMergeMode(reconciling, "ws_get_messages")).toBe(
			"authoritative",
		);
	});

	it("tracks sync revision lifecycle", () => {
		const machine = createInitialChatStateMachine("oqto-abc");
		const syncing = beginMessageSync(machine, "history");
		expect(syncing.sync.phase).toBe("syncing");
		expect(syncing.sync.lastSource).toBe("history");
		const done = completeMessageSync(syncing, "history");
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
