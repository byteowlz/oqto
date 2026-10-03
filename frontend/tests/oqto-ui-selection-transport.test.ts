import { afterEach, describe, expect, it, vi } from "vitest";
import { createChatTransport } from "../src/oqto-ui/engine/transport";
import type {
	SocketLike,
	WireEvent,
} from "../src/oqto-ui/engine/transport-contract";
import { createLiveSelectionHost } from "../src/oqto-ui/platform/live-selection";
import { review, settle } from "./selection-fixtures";

class Socket implements SocketLike {
	onOpen: (() => void) | null = null;
	onMessage: ((data: string) => void) | null = null;
	onClose: (() => void) | null = null;
	sent: Array<Record<string, unknown>> = [];
	send(data: string) {
		this.sent.push(JSON.parse(data));
	}
	close() {}
	receive(value: WireEvent) {
		this.onMessage?.(JSON.stringify(value));
	}
	reply(command: string, data: object | null, success = true) {
		const request = this.sent.findLast((row) => row.cmd === command);
		if (!request) throw new Error(`Missing request ${command}`);
		this.receive({
			channel: "agent",
			event: "response",
			cmd: command,
			id: String(request.id),
			session_id: String(request.session_id),
			success,
			...(data === null ? {} : { data: JSON.parse(JSON.stringify(data)) }),
		});
	}
}
function harness() {
	vi.useFakeTimers();
	const sockets: Socket[] = [];
	const outbox = vi.fn();
	const transport = createChatTransport({
		sockets: () => {
			const socket = new Socket();
			sockets.push(socket);
			return socket;
		},
		outbox: { load: () => [], save: outbox },
		clock: {
			schedule: (callback, delay) => setTimeout(callback, delay),
			cancel: (handle) => clearTimeout(handle),
			now: () => Date.now(),
		},
	});
	const timeline = vi.fn();
	transport.attach("platform", timeline);
	const socket = sockets[0];
	socket.onOpen?.();
	const controller = createLiveSelectionHost(transport).session("platform");
	const stop = controller.subscribe(() => {});
	expect(socket.sent.some((row) => row.cmd === "get_commands")).toBe(false);
	socket.receive({
		channel: "agent",
		event: "response",
		cmd: "session.create",
		session_id: "platform",
		success: true,
	});
	return { controller, transport, socket, timeline, outbox, sockets, stop };
}
const names = [
	"presentation-bind",
	"presentation-unbind",
	"selection-get",
	"selection-save",
	"selection-submit",
	"selection-close",
	"selection-ack",
];
async function bind(h: ReturnType<typeof harness>) {
	h.socket.reply(
		"get_commands",
		names.map((name) => ({ name })),
	);
	await settle();
	h.socket.reply("extension_control", {
		request_id: "runner-request",
		operation: "presentation_bind",
		binding_id: "own-binding",
		native_session_id: "native-scope",
		reply: {
			version: 1,
			command: "presentation-bind",
			// Native extension correlation is deliberately distinct from mux/runner correlation.
			requestId: "extension-request",
			ok: true,
			snapshot: {
				scopeId: "native-scope",
				bindings: [
					{
						id: "own-binding",
						clientKind: "oqto-web",
						expiresAt: Date.now() + 60000,
					},
				],
			},
		},
	});
	await settle();
}
afterEach(() => {
	window.sessionStorage.clear();
	vi.useRealTimers();
});
describe("authenticated selection transport seam", () => {
	it("waits for readiness/discovery and never injects controls into oqto-log timeline/outbox", async () => {
		const h = harness();
		await bind(h);
		expect(h.controller.snapshot().phase).toBe("ready");
		h.socket.receive({
			channel: "agent",
			event: "status",
			session_id: "other-session",
			key: "pi-selection:present/v1",
			text: JSON.stringify({
				version: 1,
				bindingIds: ["own-binding"],
				review: review(),
			}),
		});
		expect(h.controller.snapshot().record).toBeNull();
		h.socket.receive({
			channel: "agent",
			event: "status",
			session_id: "platform",
			key: "pi-selection:present/v1",
			text: JSON.stringify({
				version: 1,
				bindingIds: ["own-binding"],
				review: review(),
			}),
		});
		expect(h.controller.snapshot().record?.scopeId).toBe("native-scope");
		expect(h.timeline).toHaveBeenCalled();
		h.controller.rendered("review-one");
		const ack = h.socket.sent.at(-1);
		expect(ack).toMatchObject({
			cmd: "extension_control",
			operation: "selection_ack",
			arguments: { id: "review-one" },
		});
		expect(
			h.socket.sent.some((row) =>
				["prompt", "steer", "follow_up"].includes(String(row.cmd)),
			),
		).toBe(false);
		expect(h.outbox).not.toHaveBeenCalled();
		h.stop();
		h.transport.disconnect();
	});
	it("fails closed without server-owned binding identity or on denied operation", async () => {
		const h = harness();
		h.socket.reply(
			"get_commands",
			names.map((name) => ({ name })),
		);
		await settle();
		h.socket.reply("extension_control", {
			request_id: "r",
			operation: "presentation_bind",
			native_session_id: "native-scope",
			reply: {
				version: 1,
				command: "presentation-bind",
				requestId: "r",
				ok: true,
				snapshot: {
					scopeId: "native-scope",
					bindings: [
						{
							id: "forged-first-client",
							clientKind: "oqto-web",
							expiresAt: 99999,
						},
					],
				},
			},
		});
		await settle();
		expect(h.controller.snapshot().phase).toBe("unavailable");
		expect(h.controller.snapshot().error).toMatch(/lease/);
		h.stop();
		h.transport.disconnect();
	});
	it("keeps authoritative state unchanged when the backend denies mutation", async () => {
		const h = harness();
		await bind(h);
		h.socket.receive({
			channel: "agent",
			event: "status",
			session_id: "platform",
			key: "pi-selection:present/v1",
			text: JSON.stringify({
				version: 1,
				bindingIds: ["own-binding"],
				review: review(),
			}),
		});
		const request = h.controller.save();
		await settle();
		expect(h.socket.sent.at(-1)).toMatchObject({
			cmd: "extension_control",
			operation: "selection_save",
			arguments: { id: "review-one", revision: 0 },
		});
		h.socket.reply("extension_control", null, false);
		await request;
		expect(h.controller.snapshot().record?.revision).toBe(0);
		expect(h.controller.snapshot().record?.state).toBe("draft");
		expect(h.controller.snapshot().error).toBeTruthy();
		expect(h.outbox).not.toHaveBeenCalled();
		h.stop();
		h.transport.disconnect();
	});
	it("does not send an absent command and keeps the timeline's sole handler", async () => {
		const h = harness();
		h.socket.reply("get_commands", []);
		await settle();
		expect(h.controller.snapshot().phase).toBe("unavailable");
		expect(
			h.socket.sent.filter((row) => row.cmd === "extension_control"),
		).toHaveLength(0);
		h.socket.receive({
			channel: "agent",
			session_id: "platform",
			event: "message.delta",
			text: "timeline",
		});
		expect(h.timeline).toHaveBeenCalledWith(
			expect.objectContaining({ event: "message.delta" }),
		);
		h.stop();
		h.transport.disconnect();
	});
	it("rejects successful-but-miscorrelated control envelopes and never trusts Status reply claims", async () => {
		const h = harness();
		h.socket.reply(
			"get_commands",
			names.map((name) => ({ name })),
		);
		await settle();
		h.socket.receive({
			channel: "agent",
			session_id: "platform",
			event: "status",
			key: "pi-capabilities:reply/v1",
			text: JSON.stringify({ ok: true }),
		});
		expect(h.controller.snapshot().phase).toBe("connecting");
		h.socket.reply("extension_control", {
			request_id: "r",
			operation: "selection_submit",
			native_session_id: "native-scope",
			reply: { ok: true },
		});
		await settle();
		expect(h.controller.snapshot().phase).toBe("unavailable");
		h.stop();
		h.transport.disconnect();
	});
});
