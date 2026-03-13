import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";

vi.mock("@/lib/control-plane-client", () => ({
	controlPlaneApiUrl: () => "http://localhost:8080/api/ws/mux",
	getAuthToken: () => null,
}));

import { destroyWsManager, getWsManager } from "@/lib/ws-manager";

class FakeWebSocket {
	static CONNECTING = 0;
	static OPEN = 1;
	static CLOSING = 2;
	static CLOSED = 3;
	static instances: FakeWebSocket[] = [];

	readyState = FakeWebSocket.CONNECTING;
	onopen: (() => void) | null = null;
	onclose: ((event: { code: number; reason: string }) => void) | null = null;
	onmessage: ((event: { data: string }) => void) | null = null;
	onerror: ((event: unknown) => void) | null = null;
	sent: string[] = [];

	constructor(_url: string) {
		FakeWebSocket.instances.push(this);
		queueMicrotask(() => {
			this.readyState = FakeWebSocket.OPEN;
			this.onopen?.();
		});
	}

	send(data: string) {
		this.sent.push(data);
	}

	close(code = 1000, reason = "") {
		this.readyState = FakeWebSocket.CLOSED;
		this.onclose?.({ code, reason });
	}
}

describe("ws-manager reliability", () => {
	beforeEach(() => {
		FakeWebSocket.instances = [];
		(globalThis as unknown as { WebSocket: typeof FakeWebSocket }).WebSocket =
			FakeWebSocket;
		destroyWsManager();
	});

	afterEach(() => {
		destroyWsManager();
	});

	it("correlates command failures via system.error id", async () => {
		const manager = getWsManager();

		const pending = manager.sendAndWait(
			{
				channel: "files",
				type: "tree",
				path: ".",
				workspace_path: "/tmp/ws",
			},
			1000,
		);

		await Promise.resolve();
		await Promise.resolve();

		const ws = FakeWebSocket.instances[0];
		expect(ws).toBeDefined();

		for (let i = 0; i < 20 && ws.sent.length === 0; i += 1) {
			await new Promise((resolve) => setTimeout(resolve, 0));
		}
		expect(ws.sent.length).toBeGreaterThan(0);

		const sent = JSON.parse(ws.sent[0]) as { id: string };
		expect(sent.id).toBeTruthy();

		ws.onmessage?.({
			data: JSON.stringify({
				channel: "system",
				type: "error",
				id: sent.id,
				error: "Command timed out",
			}),
		});

		await expect(pending).resolves.toMatchObject({
			channel: "system",
			type: "error",
			id: sent.id,
		});
	});
});
