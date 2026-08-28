import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createChatTransport } from "../src/oqto-ui/chat/engine/transport";
import type { ChatTransport } from "../src/oqto-ui/chat/engine/transport";
import type {
	OutboxEntry,
	SocketLike,
	WireEvent,
} from "../src/oqto-ui/chat/engine/transport-contract";

class FakeSocket implements SocketLike {
	sent: Array<Record<string, unknown>> = [];
	closed = false;
	onOpen: (() => void) | null = null;
	onMessage: ((data: string) => void) | null = null;
	onClose: (() => void) | null = null;

	send(data: string): void {
		this.sent.push(JSON.parse(data) as Record<string, unknown>);
	}
	close(): void {
		this.closed = true;
	}
	open(): void {
		this.onOpen?.();
	}
	receive(event: WireEvent): void {
		this.onMessage?.(JSON.stringify(event));
	}
	drop(): void {
		this.onClose?.();
	}
	sentCmds(): string[] {
		return this.sent.map((c) => String(c.cmd ?? c.type));
	}
}

function harness(outbox: OutboxEntry[] = []) {
	const sockets: FakeSocket[] = [];
	const saved: OutboxEntry[][] = [];
	const transport = createChatTransport({
		sockets: () => {
			const socket = new FakeSocket();
			sockets.push(socket);
			return socket;
		},
		outbox: { load: () => outbox, save: (entries) => saved.push(entries) },
		clock: {
			schedule: (callback, delayMs) => setTimeout(callback, delayMs),
			cancel: (handle) => clearTimeout(handle),
			now: () => Date.now(),
		},
	});
	return {
		transport,
		sockets,
		saved,
		latest: () => sockets.at(-1) as FakeSocket,
	};
}

function readySession(
	transport: ChatTransport,
	socket: FakeSocket,
	sessionId: string,
): WireEvent[] {
	const events: WireEvent[] = [];
	transport.attach(sessionId, (event) => events.push(event));
	socket.receive({
		channel: "agent",
		event: "response",
		cmd: "session.create",
		success: true,
		session_id: sessionId,
	});
	return events;
}

beforeEach(() => {
	vi.useFakeTimers();
});
afterEach(() => {
	vi.useRealTimers();
});

describe("chat transport", () => {
	it("creates the session on attach and gates sends until ready", () => {
		const { transport, latest } = harness();
		transport.attach("s1", () => {});
		latest().open();
		expect(latest().sentCmds()).toContain("session.create");

		transport.sendMessage("s1", "prompt", "hello");
		expect(latest().sentCmds()).not.toContain("prompt");

		latest().receive({
			channel: "agent",
			event: "response",
			cmd: "session.create",
			success: true,
			session_id: "s1",
		});
		expect(transport.status("s1").sessionReady).toBe(true);
		const prompt = latest().sent.find((c) => c.cmd === "prompt");
		expect(prompt).toMatchObject({ session_id: "s1", message: "hello" });
	});

	it("routes events to the single session owner; replacement wins", () => {
		const { transport, latest } = harness();
		transport.attach("s1", () => {});
		latest().open();
		const first: WireEvent[] = [];
		const second: WireEvent[] = [];
		const detachFirst = transport.attach("s1", (e) => first.push(e));
		transport.attach("s1", (e) => second.push(e));
		detachFirst(); // stale detach must not remove the new owner
		latest().receive({
			channel: "agent",
			event: "stream.text_delta",
			session_id: "s1",
			delta: "x",
		});
		expect(first).toHaveLength(0);
		expect(second).toHaveLength(1);
	});

	it("acks clear the outbox; unacked sends retry then give up", () => {
		const { transport, latest, saved } = harness();
		transport.attach("s1", () => {});
		latest().open();
		readySession(transport, latest(), "s1");

		const id = transport.sendMessage("s1", "steer", "important");
		expect(saved.at(-1)).toHaveLength(1);

		// No ack: retries re-send the same command id.
		vi.advanceTimersByTime(5100);
		const resends = latest().sent.filter((c) => c.cmd === "steer");
		expect(resends).toHaveLength(2);
		expect(resends[1].id).toBe(id);

		// Ack clears outbox and stops retries.
		latest().receive({
			channel: "agent",
			event: "response",
			cmd: "steer",
			success: true,
			id,
			session_id: "s1",
		});
		expect(saved.at(-1)).toHaveLength(0);
		vi.advanceTimersByTime(20000);
		expect(latest().sent.filter((c) => c.cmd === "steer")).toHaveLength(2);
	});

	it("reconnects with backoff, recreates sessions, and signals resync", () => {
		const { transport, sockets, latest } = harness();
		transport.attach("s1", () => {});
		latest().open();
		readySession(transport, latest(), "s1");
		const resyncs: string[] = [];
		transport.observe(null, (sessionId) => resyncs.push(sessionId));

		latest().drop();
		expect(transport.status().connection).toBe("reconnecting");
		expect(transport.status("s1").sessionReady).toBe(false);

		vi.advanceTimersByTime(1000);
		expect(sockets).toHaveLength(2);
		latest().open();
		expect(transport.status().connection).toBe("connected");
		expect(latest().sentCmds()).toContain("session.create");

		vi.advanceTimersByTime(300);
		expect(resyncs).toEqual(["s1"]);
	});

	it("queues messages while disconnected and flushes after ready", () => {
		const { transport, latest } = harness();
		transport.attach("s1", () => {});
		latest().open();
		readySession(transport, latest(), "s1");
		latest().drop();

		transport.sendMessage("s1", "prompt", "typed offline");
		vi.advanceTimersByTime(1000);
		latest().open();
		expect(latest().sentCmds()).toContain("session.create");
		expect(latest().sentCmds()).not.toContain("prompt");

		latest().receive({
			channel: "agent",
			event: "response",
			cmd: "session.create",
			success: true,
			session_id: "s1",
		});
		const prompt = latest().sent.find((c) => c.cmd === "prompt");
		expect(prompt).toMatchObject({ message: "typed offline" });
	});

	it("restores a persisted outbox and sends it once the session is ready", () => {
		const { transport, latest } = harness([
			{
				id: "req-old",
				sessionId: "s1",
				cmd: "prompt",
				message: "from last tab",
				updatedAt: Date.now(),
			},
		]);
		transport.attach("s1", () => {});
		latest().open();
		readySession(transport, latest(), "s1");
		const prompt = latest().sent.find((c) => c.cmd === "prompt");
		expect(prompt).toMatchObject({ id: "req-old", message: "from last tab" });
	});

	it("expires stale outbox entries instead of replaying them", () => {
		const { transport, latest } = harness([
			{
				id: "req-ancient",
				sessionId: "s1",
				cmd: "prompt",
				message: "very old",
				updatedAt: Date.now() - 11 * 60 * 1000,
			},
		]);
		transport.attach("s1", () => {});
		latest().open();
		readySession(transport, latest(), "s1");
		expect(latest().sent.find((c) => c.cmd === "prompt")).toBeUndefined();
	});

	it("resolves correlated requests and times them out", async () => {
		const { transport, latest } = harness();
		transport.attach("s1", () => {});
		latest().open();
		readySession(transport, latest(), "s1");

		const pending = transport.request("s1", "get_state");
		const sent = latest().sent.find((c) => c.cmd === "get_state");
		expect(sent?.id).toBeDefined();
		latest().receive({
			channel: "agent",
			event: "response",
			cmd: "get_state",
			success: true,
			id: String(sent?.id),
			session_id: "s1",
		});
		await expect(pending).resolves.toMatchObject({ cmd: "get_state" });

		const timedOut = transport.request("s1", "get_state", undefined, 1000);
		const rejection = expect(timedOut).rejects.toThrow("Request timeout");
		vi.advanceTimersByTime(1100);
		await rejection;
	});

	it("stops reconnecting after disconnect() and closes the socket", () => {
		const { transport, sockets, latest } = harness();
		transport.attach("s1", () => {});
		latest().open();
		transport.disconnect();
		expect(latest().closed).toBe(true);
		vi.advanceTimersByTime(60000);
		expect(sockets).toHaveLength(1);
		expect(transport.status().connection).toBe("disconnected");
	});
});
