/**
 * Deterministic fake agent peer for the scripted adapter: a socket server
 * that answers session lifecycle, persists prompts like a real backend
 * would, and streams a canned assistant turn. Drives the REAL transport +
 * projection stack so the dev route exercises the production path.
 */

import type { WireEvent } from "../engine/transport-contract";
import type { ChatMessage } from "../platform/contracts";

export type ScriptedServerOptions = {
	/** Called when a prompt is durably accepted (user message persisted). */
	onPersisted?: (sessionId: string, message: ChatMessage) => void;
	/** Called when the assistant turn completes (reply persisted). */
	onTurnCompleted?: (sessionId: string, message: ChatMessage) => void;
	delayMs?: number;
};

let counter = 0;

type ServerCommand = {
	id?: string;
	session_id?: string;
	cmd?: string;
	message?: string;
};

export class ScriptedChatServer {
	private readonly timers: ReturnType<typeof setTimeout>[] = [];

	constructor(
		private readonly push: (event: WireEvent) => void,
		private readonly options: ScriptedServerOptions = {},
	) {}

	private emit(event: WireEvent): void {
		const timer = setTimeout(() => {
			this.push(event);
		}, this.options.delayMs ?? 15);
		this.timers.push(timer);
	}

	private reply(command: ServerCommand): void {
		this.emit({
			channel: "agent",
			event: "response",
			cmd: command.cmd,
			id: command.id,
			success: true,
			session_id: command.session_id,
		});
	}

	handleMessage(data: string): void {
		let command: {
			channel?: string;
			cmd?: string;
			session_id?: string;
			id?: string;
			message?: string;
		};
		try {
			command = JSON.parse(data) as typeof command;
		} catch {
			return;
		}
		if (command.channel !== "agent" || !command.session_id) return;
		const sessionId = command.session_id;

		switch (command.cmd) {
			case "session.create": {
				this.reply({ ...command, session_id: sessionId });
				this.emit({
					channel: "agent",
					event: "session.created",
					session_id: sessionId,
				});
				return;
			}
			case "prompt":
			case "steer":
			case "follow_up": {
				this.reply({
					id: command.id,
					cmd: command.cmd,
					session_id: sessionId,
				});
				this.runTurn(sessionId, command.message ?? "");
				return;
			}
			case "abort": {
				this.reply({
					id: command.id,
					cmd: "abort",
					session_id: sessionId,
				});
				this.emit({
					channel: "agent",
					event: "stream.done",
					session_id: sessionId,
				});
				return;
			}
			default:
				this.reply({ ...command, session_id: sessionId });
		}
	}

	private runTurn(sessionId: string, prompt: string): void {
		const turn = ++counter;
		const userMessage: ChatMessage = {
			id: `scripted-user-${turn}`,
			author: "user",
			content: prompt,
			time: "now",
		};
		this.options.onPersisted?.(sessionId, userMessage);

		this.emit({
			channel: "agent",
			event: "stream.message_start",
			session_id: sessionId,
			role: "assistant",
		});
		const opener = `Working on: “${prompt.slice(0, 80)}”`;
		this.emit({
			channel: "agent",
			event: "stream.text_delta",
			session_id: sessionId,
			delta: opener,
		});
		this.emit({
			channel: "agent",
			event: "tool.start",
			session_id: sessionId,
			tool_call_id: `call-${turn}`,
			name: "read",
		});
		this.emit({
			channel: "agent",
			event: "tool.end",
			session_id: sessionId,
			tool_call_id: `call-${turn}`,
			name: "read",
			is_error: false,
			output: "ok",
		});
		this.emit({
			channel: "agent",
			event: "stream.text_delta",
			session_id: sessionId,
			delta:
				" The scripted engine streamed this through the real transport and projection.",
		});
		this.emit({
			channel: "agent",
			event: "stream.done",
			session_id: sessionId,
		});

		const assistantMessage: ChatMessage = {
			id: `scripted-assistant-${turn}`,
			author: "agent",
			content: `${opener} The scripted engine streamed this through the real transport and projection.`,
			time: "now",
		};
		this.options.onTurnCompleted?.(sessionId, assistantMessage);
	}

	dispose(): void {
		for (const timer of this.timers) clearTimeout(timer);
	}
}
