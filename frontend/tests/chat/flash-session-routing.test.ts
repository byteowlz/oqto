import type { ConversationView } from "@oqto-flash/types";
import { beforeEach, describe, expect, it, vi } from "vitest";
const state = vi.hoisted(() => ({
	peers: new Set<{
		target: string;
		publish: (view: ConversationView) => void;
	}>(),
	views: new Map<string, ConversationView>(),
	opened: [] as string[],
	closed: [] as string[],
	sent: [] as { target: string; text: string; requestId: string }[],
	filesRevision: 0,
}));
const id = (n: number) =>
	`00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
const directory = "10000000-0000-4000-8000-000000000001";
function publish(target: string, value = state.views.get(target)) {
	if (!value) throw new Error("Missing synthetic native view");
	state.views.set(target, value);
	for (const peer of state.peers)
		if (peer.target === target) peer.publish(value);
}
vi.mock("@oqto-flash/worker-client", () => ({
	FlashWorkerClient: class {
		peer: { target: string; publish: (view: ConversationView) => void };
		constructor(callback: (view: ConversationView) => void) {
			this.peer = { target: "", publish: callback };
		}
		get filesRevision() {
			return state.filesRevision;
		}
		async open(
			_workspace: string,
			_binding: unknown,
			_key: string,
			target = id(1),
		) {
			if (!state.views.has(target)) throw new Error("Unknown native target");
			this.peer.target = target;
			state.opened.push(target);
			state.peers.add(this.peer);
			publish(target);
			return { publicSessionId: target, workDirectoryId: directory };
		}
		async sessions() {
			return [...state.views.keys()].map((publicSessionId) => ({
				publicSessionId,
				workDirectoryId: directory,
				name: publicSessionId,
				modelId: "first",
				status: "idle",
			}));
		}
		async selectModel(modelId: string) {
			const view = state.views.get(this.peer.target);
			if (!view) throw new Error("Missing native target");
			publish(this.peer.target, {
				...view,
				docs: {
					...view.docs,
					"pi.agent": { model: { provider: "flash", modelId } },
				},
			});
		}
		async submit(text: string, requestId: string) {
			state.sent.push({ target: this.peer.target, text, requestId });
		}
		async abort() {}
		async close() {
			state.closed.push(this.peer.target);
			state.peers.delete(this.peer);
		}
	},
}));
import { createFlashChat } from "../../src/oqto-ui/platform/flash-chat";
const options = {
	binding: {
		api: "openai-completions",
		baseUrl: "https://fixture.invalid/v1",
		label: "fixture",
		model: "first",
	},
	apiKey: "fixture-only",
};
beforeEach(() => {
	state.peers.clear();
	state.views.clear();
	state.opened = [];
	state.closed = [];
	state.sent = [];
	state.filesRevision = 0;
	for (let n = 1; n <= 12; n++)
		state.views.set(id(n), {
			conversation: { id: n },
			entries: [
				{
					id: n,
					conversationId: n,
					kind: "message",
					model: [{ role: "user", content: "identical", timestamp: 0 }],
				},
			],
			docs: { "pi.agent": { model: { provider: "flash", modelId: "first" } } },
		} as unknown as ConversationView);
});
describe("Flash workspace projection routing", () => {
	it("keeps peer history, updates, submissions and model ACKs bound to their original public target", async () => {
		const runtime = createFlashChat(options);
		await runtime.open();
		await runtime.sessions();
		await runtime.ensureSession(id(2));
		const rootUpdates = vi.fn();
		const otherUpdates = vi.fn();
		const stopRoot = runtime.chat.bind(id(1), rootUpdates);
		const stopOther = runtime.chat.bind(id(2), otherUpdates);
		rootUpdates.mockClear();
		otherUpdates.mockClear();
		const second = state.views.get(id(2));
		if (!second) throw new Error("Missing fixture");
		publish(id(2), {
			...second,
			docs: {
				...second.docs,
				"pi.live": {
					run: { taskId: 9 },
					generation: {
						attempt: 1,
						message: { content: [{ type: "text", text: "Only second draft" }] },
					},
				},
			},
		});
		expect(rootUpdates).not.toHaveBeenCalled();
		expect(otherUpdates).toHaveBeenCalled();
		expect(runtime.getSnapshot()).toBe(state.views.get(id(1)));
		const rootRequest = runtime.chat.send(id(1), "root request", "prompt");
		const otherRequest = runtime.chat.send(id(2), "other request", "prompt");
		await vi.waitFor(() => expect(state.sent).toHaveLength(2));
		expect(state.sent).toEqual([
			{ target: id(1), text: "root request", requestId: rootRequest },
			{ target: id(2), text: "other request", requestId: otherRequest },
		]);
		await runtime.selectModel(id(2), "second");
		expect(state.views.get(id(1))?.docs["pi.agent"].model).toEqual({
			provider: "flash",
			modelId: "first",
		});
		expect(state.views.get(id(2))?.docs["pi.agent"].model).toEqual({
			provider: "flash",
			modelId: "second",
		});
		const firstHistory = await runtime.loadMessages(id(1));
		const otherHistory = await runtime.loadMessages(id(2));
		expect(firstHistory.messages[0].content).toBe(
			otherHistory.messages[0].content,
		);
		expect(firstHistory.messages[0].id).not.toBe(otherHistory.messages[0].id);
		await expect(runtime.ensureSession(id(99))).rejects.toThrow("Unknown");
		stopOther();
		stopRoot();
		await runtime.dispose();
		expect(state.closed.at(-1)).toBe(id(1));
	});
	it("evicts only unbound projection peers, retains the owner anchor and reopens an old cached route safely", async () => {
		const runtime = createFlashChat(options);
		await runtime.open();
		await runtime.sessions();
		for (let n = 2; n <= 10; n++) await runtime.ensureSession(id(n));
		expect(state.peers.size).toBe(8);
		expect(state.closed).toContain(id(2));
		expect(state.closed).not.toContain(id(1));
		const update = vi.fn();
		const stop = runtime.chat.bind(id(2), update);
		await vi.waitFor(() =>
			expect(update).toHaveBeenCalledWith({ kind: "resync" }),
		);
		expect(state.opened.filter((value) => value === id(2))).toHaveLength(2);
		expect(runtime.getIdentity(id(2)).publicSessionId).toBe(id(2));
		expect(runtime.getSnapshot()).toBe(state.views.get(id(1)));
		const workspaceChanges = vi.fn();
		const unwatch = runtime.subscribe(workspaceChanges);
		state.filesRevision++;
		publish(id(1));
		expect(workspaceChanges).toHaveBeenCalled();
		expect(runtime.getFilesRevision()).toBe(1);
		stop();
		unwatch();
		await runtime.dispose();
		expect(state.peers.size).toBe(0);
	});
});
