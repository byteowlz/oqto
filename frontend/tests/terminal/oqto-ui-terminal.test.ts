import type { MuxSocket } from "@/src/oqto-ui/platform/mux-files";
import { createMuxTerminalHost } from "@/src/oqto-ui/platform/mux-terminal";
import { describe, expect, it, vi } from "vitest";

function fakeSocket() {
	const sent: Record<string, unknown>[] = [];
	const socket: MuxSocket & {
		open(): void;
		deliver(event: object): void;
		closed: boolean;
	} = {
		closed: false,
		send: (data) => sent.push(JSON.parse(data) as Record<string, unknown>),
		close: () => {
			socket.closed = true;
			socket.onclose?.call(null, null);
		},
		onopen: null,
		onmessage: null,
		onclose: null,
		open: () => socket.onopen?.call(null, null),
		deliver: (event) =>
			socket.onmessage?.call(null, { data: JSON.stringify(event) }),
	};
	return { socket, sent };
}

const OPENED = { channel: "terminal", type: "opened", terminal_id: "pty-1" };

describe("terminal adapter", () => {
	it("opens a PTY, then carries the host's id on every later command", async () => {
		const { socket, sent } = fakeSocket();
		const host = createMuxTerminalHost(() => socket);
		const pending = host.open("/work/repo", { cols: 100, rows: 30 }, () => {});
		socket.open();
		expect(sent[0]).toEqual({
			channel: "terminal",
			type: "open",
			workspace_path: "/work/repo",
			cols: 100,
			rows: 30,
		});
		socket.deliver(OPENED);
		// The host confirms more than once in practice; the first id wins.
		socket.deliver({ ...OPENED, terminal_id: "pty-2" });
		const session = await pending;
		session.input("ls\r");
		session.resize({ cols: 120, rows: 40 });
		expect(sent[1]).toEqual({
			channel: "terminal",
			type: "input",
			terminal_id: "pty-1",
			data: "ls\r",
		});
		expect(sent[2]).toEqual({
			channel: "terminal",
			type: "resize",
			terminal_id: "pty-1",
			cols: 120,
			rows: 40,
		});
	});

	it("decodes output as raw bytes rather than text", async () => {
		const { socket } = fakeSocket();
		const host = createMuxTerminalHost(() => socket);
		const onOutput = vi.fn();
		const pending = host.open("/work/repo", { cols: 80, rows: 24 }, onOutput);
		socket.open();
		socket.deliver(OPENED);
		await pending;
		// An escape sequence has to survive as bytes, not as decoded text.
		const payload = `ok${String.fromCharCode(27)}[0m`;
		socket.deliver({
			channel: "terminal",
			type: "output",
			terminal_id: "pty-1",
			data_base64: btoa(payload),
		});
		expect(onOutput).toHaveBeenCalledWith(
			new Uint8Array([111, 107, 27, 91, 48, 109]),
		);
	});

	it("closes the PTY and the socket, and fails an open that never lands", async () => {
		const { socket, sent } = fakeSocket();
		const host = createMuxTerminalHost(() => socket);
		const pending = host.open("/work/repo", { cols: 80, rows: 24 }, () => {});
		socket.open();
		socket.deliver(OPENED);
		const session = await pending;
		session.close();
		expect(sent.at(-1)).toEqual({
			channel: "terminal",
			type: "close",
			terminal_id: "pty-1",
		});
		expect(socket.closed).toBe(true);

		const second = fakeSocket();
		const failing = createMuxTerminalHost(() => second.socket);
		const never = failing.open("/work/repo", { cols: 80, rows: 24 }, () => {});
		second.socket.open();
		second.socket.close();
		await expect(never).rejects.toThrow("terminal socket closed");
	});

	it("reports a host error instead of hanging", async () => {
		const { socket } = fakeSocket();
		const host = createMuxTerminalHost(() => socket);
		const pending = host.open("/work/repo", { cols: 80, rows: 24 }, () => {});
		socket.open();
		socket.deliver({ channel: "terminal", type: "error", error: "no pty" });
		await expect(pending).rejects.toThrow("no pty");
	});
});
