import type { ConversationView } from "@oqto-flash/types";
import { describe, expect, it, vi } from "vitest";
const state = vi.hoisted(() => ({
	publish: (_view: ConversationView) => {},
	submit: vi.fn(),
	abort: vi.fn(),
	selectModel: vi.fn(),
	close: vi.fn(),
}));
vi.mock("@oqto-flash/worker-client", () => ({
	FlashWorkerClient: class {
		constructor(publish: (view: ConversationView) => void) {
			state.publish = publish;
		}
		async open() {
			state.publish(view());
			return {
				publicSessionId: FLASH_SESSION_ID,
				workDirectoryId: "550e8400-e29b-41d4-a716-446655440001",
			};
		}
		async sessions() {
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
		submit = state.submit;
		abort = state.abort;
		selectModel = state.selectModel;
		close = state.close;
		async files() {
			return ["a.ts"];
		}
		async read() {
			return "native file";
		}
	},
}));
import {
	createFlashChat,
	flashHistory,
} from "../../src/oqto-ui/platform/flash-chat";
const FLASH_SESSION_ID = "550e8400-e29b-41d4-a716-446655440000";
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
	} as unknown as ConversationView;
}
describe("Flash native chat adapter", () => {
	it("waits for the authoritative model snapshot without starting a phantom turn", async () => {
		state.selectModel.mockResolvedValue(undefined);
		const runtime = createFlashChat({
			binding: {
				api: "openai-completions",
				baseUrl: "https://fixture.invalid/v1",
				label: "fixture",
				model: "first",
			},
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
		} as unknown as ConversationView;
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
		} as unknown as ConversationView;
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
		} as unknown as ConversationView;
		expect(flashHistory(failed)[0].parts?.[0]).toMatchObject({
			status: "error",
		});
	});
	it("projects equal text with distinct native identities and no shadow messages", async () => {
		const handle = createFlashChat({
			binding: {} as never,
			apiKey: "test",
			workspace: "isolated",
		});
		await handle.open();
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
		expect(await handle.files()).toEqual(["a.ts"]);
		expect(await handle.read("a.ts")).toBe("native file");
		await handle.dispose();
	});
	it("resyncs native replacements and emits draft/ended using run identity", async () => {
		const handle = createFlashChat({ binding: {} as never, apiKey: "test" });
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
