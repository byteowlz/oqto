import { beforeEach, describe, expect, it, vi } from "vitest";

const sendMock = vi.fn();
const subscribeMock = vi.fn();

vi.mock("@/lib/ws-manager", () => ({
	getWsManager: () => ({
		send: sendMock,
		subscribe: subscribeMock,
	}),
}));

import { busPublish, busSubscribe } from "@/lib/bus-client";

describe("bus-client", () => {
	beforeEach(() => {
		sendMock.mockReset();
		subscribeMock.mockReset();
	});

	it("sends publish command with mapped fields", () => {
		busPublish({
			scope: "session",
			scopeId: "ses_123",
			topic: "app.submit",
			payload: { action: "submit" },
			v: 2,
			priority: "high",
			ttlMs: 5000,
			idempotencyKey: "idem-1",
			correlationId: "corr-1",
			ack: { replyTo: "admin.ack", timeoutMs: 1500 },
		});

		expect(sendMock).toHaveBeenCalledTimes(1);
		const cmd = sendMock.mock.calls[0][0] as Record<string, unknown>;
		expect(cmd.channel).toBe("bus");
		expect(cmd.type).toBe("publish");
		expect(cmd.scope).toBe("session");
		expect(cmd.scope_id).toBe("ses_123");
		expect(cmd.topic).toBe("app.submit");
		expect(cmd.payload).toEqual({ action: "submit" });
		expect(cmd.v).toBe(2);
		expect(cmd.priority).toBe("high");
		expect(cmd.ttl_ms).toBe(5000);
		expect(cmd.idempotency_key).toBe("idem-1");
		expect(cmd.correlation_id).toBe("corr-1");
		expect(cmd.ack).toEqual({ reply_to: "admin.ack", timeout_ms: 1500 });
	});

	it("subscribes and forwards matching bus events", () => {
		let wsHandler:
			| ((event: Record<string, unknown>) => void)
			| undefined;
		const wsUnsub = vi.fn();
		subscribeMock.mockImplementation(
			(_channel: string, handler: (event: Record<string, unknown>) => void) => {
				wsHandler = handler;
				return wsUnsub;
			},
		);

		const onEvent = vi.fn();
		const sub = busSubscribe(
			{
				scope: "session",
				scopeId: "ses_1",
				topics: ["app.*"],
			},
			onEvent,
		);

		expect(sendMock).toHaveBeenCalledTimes(1);
		expect(sendMock.mock.calls[0][0]).toMatchObject({
			channel: "bus",
			type: "subscribe",
			scope: "session",
			scope_id: "ses_1",
			topics: ["app.*"],
		});

		expect(wsHandler).toBeDefined();

		// matching event -> delivered
		wsHandler?.({
			channel: "bus",
			type: "event",
			event_id: "evt_1",
			scope: "session",
			scope_id: "ses_1",
			topic: "app.submit",
			payload: { ok: true },
			source: { type: "frontend", user_id: "alice" },
			ts: Date.now(),
			v: 1,
		});
		expect(onEvent).toHaveBeenCalledTimes(1);

		// different scope_id -> ignored
		wsHandler?.({
			channel: "bus",
			type: "event",
			event_id: "evt_2",
			scope: "session",
			scope_id: "ses_other",
			topic: "app.submit",
			payload: { ok: false },
			source: { type: "frontend", user_id: "alice" },
			ts: Date.now(),
			v: 1,
		});
		expect(onEvent).toHaveBeenCalledTimes(1);

		sub.unsubscribe();
		expect(wsUnsub).toHaveBeenCalledTimes(1);
		expect(sendMock).toHaveBeenCalledTimes(2);
		expect(sendMock.mock.calls[1][0]).toMatchObject({
			channel: "bus",
			type: "unsubscribe",
			scope: "session",
			scope_id: "ses_1",
			topics: ["app.*"],
		});
	});
});
