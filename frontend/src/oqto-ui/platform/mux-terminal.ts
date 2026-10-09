/**
 * Live PTY over the multiplexed WebSocket terminal channel. One socket per
 * session, opened lazily; the host answers `opened` with the id every later
 * command carries, and streams output as base64 of the raw bytes.
 */

import type { MuxSocket } from "./mux-files";
import type {
	TerminalHost,
	TerminalSession,
	TerminalSize,
} from "./terminal-contract";

interface TerminalEvent {
	readonly channel?: string;
	readonly type?: string;
	readonly terminal_id?: string;
	readonly data_base64?: string;
	readonly error?: string;
}

/** A terminal-channel command; the channel tag is added when framing. */
interface TerminalCommand {
	readonly type: string;
	readonly terminal_id: string;
	readonly data?: string;
	readonly cols?: number;
	readonly rows?: number;
}

function decodeBytes(encoded: string): Uint8Array {
	const binary = atob(encoded);
	return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

export function createMuxTerminalHost(
	openSocket: () => MuxSocket,
): TerminalHost {
	return {
		open(workspacePath, size, onOutput) {
			const socket = openSocket();
			let terminalId: string | null = null;
			const queue: string[] = [];
			const send = (payload: TerminalCommand) => {
				const frame = JSON.stringify({ channel: "terminal", ...payload });
				if (terminalId) socket.send(frame);
				else queue.push(frame);
			};
			return new Promise<TerminalSession>((resolve, reject) => {
				socket.onopen = () => {
					socket.send(
						JSON.stringify({
							channel: "terminal",
							type: "open",
							workspace_path: workspacePath,
							cols: size.cols,
							rows: size.rows,
						}),
					);
				};
				socket.onmessage = (event) => {
					let message: TerminalEvent;
					try {
						message = JSON.parse(String(event.data)) as TerminalEvent;
					} catch {
						return;
					}
					if (message.channel !== "terminal") return;
					if (message.type === "error") {
						reject(new Error(message.error ?? "terminal error"));
						return;
					}
					if (message.type === "opened" && message.terminal_id) {
						// The host may confirm more than once; the first id wins.
						if (terminalId) return;
						terminalId = message.terminal_id;
						for (const frame of queue.splice(0)) socket.send(frame);
						resolve({
							input: (data: string) =>
								send({ type: "input", terminal_id: terminalId ?? "", data }),
							resize: (next: TerminalSize) =>
								send({
									type: "resize",
									terminal_id: terminalId ?? "",
									cols: next.cols,
									rows: next.rows,
								}),
							close: () => {
								if (terminalId)
									send({ type: "close", terminal_id: terminalId });
								socket.close();
							},
						});
						return;
					}
					if (message.type === "output" && message.data_base64) {
						onOutput(decodeBytes(message.data_base64));
					}
				};
				socket.onclose = () => {
					if (!terminalId) reject(new Error("terminal socket closed"));
				};
			});
		},
	};
}
