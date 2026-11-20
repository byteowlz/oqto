import { describe, expect, it, vi } from "vitest";
import type {
	FlashConversationView as ConversationView,
	FlashProviderBinding,
	FlashSessionDescriptor,
	FlashWorkerFactory,
	FlashWorkerPort,
} from "../../src/oqto-ui/platform/flash-runtime-contract";
const state = vi.hoisted(() => ({
	publish: (_view: ConversationView) => {},
	open: vi.fn(),
	createSession: vi.fn(),
	submit: vi.fn(),
	abort: vi.fn(),
	selectModel: vi.fn(),
	close: vi.fn(),
}));
class SyntheticWorker implements FlashWorkerPort {
	readonly filesRevision = 0;
	constructor(publish: (view: ConversationView) => void) {
		state.publish = publish;
	}
	async open(...args: Parameters<FlashWorkerPort["open"]>) {
		state.open(...args);
		state.publish(view());
		return {
			publicSessionId: FLASH_SESSION_ID,
			workDirectoryId: "550e8400-e29b-41d4-a716-446655440001",
		};
	}
	async sessions(): Promise<FlashSessionDescriptor[]> {
		return [
			{
				publicSessionId: FLASH_SESSION_ID,
				workDirectoryId: "550e8400-e29b-41d4-a716-446655440001",
				name: "Browser Session",
				modelId: "fixture",
				status: "idle",
			},
		];
	}
	submit(...args: Parameters<FlashWorkerPort["submit"]>) {
		return Promise.resolve(state.submit(...args));
	}
	abort() {
		return Promise.resolve(state.abort());
	}
	selectModel(modelId: string) {
		return Promise.resolve(state.selectModel(modelId));
	}
	async close() {
		await state.close();
	}
	async read() {
		return "native file";
	}
	async list() {
		return [
			{
				path: "/workspace/a.ts",
				name: "a.ts",
				kind: "file",
				size: 11,
				mtimeMs: 0,
			},
		];
	}
	async createSession(
		...args: Parameters<FlashWorkerPort["createSession"]>
	): Promise<never> {
		state.createSession(...args);
		throw new Error("Synthetic native creation outcome is unknown");
	}
}
const createClient: FlashWorkerFactory = (publish) =>
	new SyntheticWorker(publish);
import {
	createFlashChat,
	flashHistory,
} from "../../src/oqto-ui/platform/flash-chat";
const FLASH_SESSION_ID = "550e8400-e29b-41d4-a716-446655440000";
const binding: FlashProviderBinding = {
	api: "openai-completions",
	baseUrl: "https://fixture.invalid/v1",
	label: "fixture",
	model: "first",
};
function view(): ConversationView {
	return {
		conversation: { id: "native-conversation" },
		entries: [1, 2, 3].map((n) => ({
			id: `entry-${n}`,
			conversationId: "native-conversation",
			kind: "message",
			model: [{ role: "user", content: "identical", timestamp: 0 }],
		})),
		// Actual ConversationView mounts built-in docs only. Identity is the
		// owner's separate native-metadata response, never a fabricated view doc.
		docs: {},
	};
}
describe("Flash native chat adapter", () => {
	it("fails closed without a host factory rather than inventing a runtime", () => {
		expect(() =>
			Reflect.apply(createFlashChat, undefined, [
				{ binding, apiKey: "synthetic" },
			]),
		).toThrow("host-owned Flash worker factory");
	});
	it("waits for the authoritative model snapshot without starting a phantom turn", async () => {
		state.selectModel.mockResolvedValue(undefined);
		const runtime = createFlashChat({
			createClient,
			binding,
			apiKey: "synthetic",
		});
		await runtime.open();
		const updates: Array<{ kind: string }> = [];
		const unbind = runtime.chat.bind(FLASH_SESSION_ID, (update) =>
			updates.push(update),
		);
		let settled = false;
		const selection = runtime
			.selectModel(FLASH_SESSION_ID, "second")
			.then(() => {
				settled = true;
			});
		await Promise.resolve();
		expect(settled).toBe(false);
		state.publish({
			...view(),
			docs: {
				...view().docs,
				"pi.agent": { model: { provider: "flash", modelId: "second" } },
			},
		});
		await selection;
		expect(state.selectModel).toHaveBeenCalledWith("second");
		expect(updates.some((update) => update.kind === "draft")).toBe(false);
		unbind();
		await runtime.dispose();
	});
	it("does not retry uncertain native creation or replace the original identity", async () => {
		const runtime = createFlashChat({
			binding,
			apiKey: "synthetic",
			createClient,
		});
		await runtime.open();
		const before = runtime.getIdentity();
		const requestId = "650e8400-e29b-41d4-a716-446655440000";
		await expect(
			runtime.createSession(FLASH_SESSION_ID, requestId),
		).rejects.toThrow("outcome is unknown");
		expect(state.createSession).toHaveBeenCalledExactlyOnceWith(
			FLASH_SESSION_ID,
			requestId,
		);
		expect(runtime.getIdentity()).toEqual(before);
		await runtime.dispose();
	});
	it("renders native provider errors instead of an empty assistant bubble", () => {
		const failed = {
			...view(),
			entries: [
				{
					...view().entries[0],
					model: [
						{
							role: "assistant",
							content: [],
							stopReason: "error",
							errorMessage: "403 scoped_key_budget_required",
						},
					],
				},
			],
		};
		expect(flashHistory(failed)[0].content).toContain(
			"scoped_key_budget_required",
		);
	});
	it("does not invent success for calls without a successful native result", () => {
		const entry = view().entries[0];
		const call = {
			...entry,
			model: [
				{
					role: "assistant",
					content: [
						{
							type: "toolCall",
							id: "native-call",
							name: "bash",
							arguments: { command: "false" },
						},
					],
				},
			],
		};
		const pending = {
			...view(),
			entries: [call],
		};
		expect(flashHistory(pending)[0].parts?.[0]).toMatchObject({
			status: "pending",
		});
		const failed = {
			...view(),
			entries: [
				call,
				{
					...entry,
					id: "native-result",
					model: [
						{
							role: "toolResult",
							toolCallId: "native-call",
							toolName: "bash",
							isError: true,
							content: "interrupted",
						},
					],
				},
			],
		};
		expect(flashHistory(failed)[0].parts?.[0]).toMatchObject({
			status: "error",
		});
	});
	it("projects equal text with distinct native identities and no shadow messages", async () => {
		const handle = createFlashChat({
			createClient,
			binding,
			apiKey: "test",
			workspace: "isolated",
		});
		await handle.open();
		expect(state.open).toHaveBeenLastCalledWith(
			"isolated",
			binding,
			"test",
			undefined,
		);
		const page = await handle.loadMessages(FLASH_SESSION_ID, undefined, 2);
		expect(page.messages.map((m) => m.id)).toEqual([
			"native-conversation:entry-2:message:0",
			"native-conversation:entry-3:message:0",
		]);
		expect(
			(
				await handle.loadMessages(
					FLASH_SESSION_ID,
					page.nextBefore ?? undefined,
				)
			).messages,
		).toHaveLength(1);
		expect(flashHistory(view())).toEqual(flashHistory(view()));
		await expect(handle.loadMessages("native-conversation")).rejects.toThrow(
			"Unknown",
		);
		await expect(
			handle.loadMessages(FLASH_SESSION_ID, "foreign"),
		).rejects.toThrow("cursor");
		for (const mode of ["prompt", "steer", "follow_up"] as const)
			handle.chat.send(FLASH_SESSION_ID, "hello", mode);
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(state.submit.mock.calls.slice(-3).map((call) => call[2])).toEqual([
			"reject",
			"steer",
			"followUp",
		]);
		expect((await handle.loadMessages(FLASH_SESSION_ID)).messages).toHaveLength(
			3,
		);
		expect(await handle.list("/workspace")).toEqual([
			{
				path: "/workspace/a.ts",
				name: "a.ts",
				kind: "file",
				size: 11,
				mtimeMs: 0,
			},
		]);
		expect(await handle.read("a.ts")).toBe("native file");
		await handle.dispose();
	});
	it("resyncs native replacements and emits draft/ended using run identity", async () => {
		const handle = createFlashChat({
			createClient,
			binding,
			apiKey: "test",
		});
		const updates: { kind: string }[] = [];
		await handle.open();
		handle.chat.bind(FLASH_SESSION_ID, (update) => updates.push(update));
		state.publish({
			...view(),
			docs: {
				...view().docs,
				"pi.live": {
					run: { taskId: "run-1", inputs: [] },
					generation: {
						attempt: 1,
						message: { content: [{ type: "text", text: "partial" }] },
					},
				},
			},
		});
		expect(updates.at(-1)).toMatchObject({
			kind: "draft",
			draft: { parts: [{ text: "partial" }] },
		});
		state.publish({ ...view(), entries: [] });
		expect(updates.at(-1)).toEqual({ kind: "ended", reason: "done" });
		expect((await handle.loadMessages(FLASH_SESSION_ID)).messages).toEqual([]);
		await handle.dispose();
	});
});
