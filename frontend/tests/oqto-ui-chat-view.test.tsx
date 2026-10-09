import { i18n, initI18n } from "@/lib/i18n";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatPane } from "../src/oqto-ui/chat/ChatPane";
import { turnDraftQueryKey } from "../src/oqto-ui/chat/query-keys";
import {
	scriptedOqtoUiPlatform,
	scriptedStore,
} from "../src/oqto-ui/dev/scripted-platform";

initI18n();
const SESSION = "chat-reliability";

beforeEach(() => {
	// Storage.getItem returns null for absent keys, never undefined.
	vi.mocked(localStorage.getItem).mockReturnValue(null);
});

describe("OqtoUI writable Chat View", () => {
	it("sends, streams through a disposable cache draft, then converges to pages", async () => {
		Object.defineProperty(window, "innerWidth", {
			configurable: true,
			value: 390,
		});
		const initialLength = scriptedStore.get(SESSION)?.length ?? 0;
		const queryClient = new QueryClient({
			defaultOptions: { queries: { retry: false } },
		});
		render(
			<QueryClientProvider client={queryClient}>
				<ChatPane
					platform={scriptedOqtoUiPlatform}
					agentName="Pi"
					sessionId={SESSION}
					tasks={[]}
					workspacePath="/workspace"
					onOpenFile={vi.fn()}
				/>
			</QueryClientProvider>,
		);

		const composer = screen.getByRole("textbox");
		fireEvent.change(composer, { target: { value: "prove writable chat" } });
		fireEvent.click(
			screen.getByRole("button", { name: i18n.t("oqtoUi.chat.send") }),
		);

		expect(screen.getByText("prove writable chat")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: i18n.t("oqtoUi.chat.abort") }),
		).toBeInTheDocument();

		await waitFor(
			() => {
				expect(scriptedStore.get(SESSION)).toHaveLength(initialLength + 2);
			},
			{ timeout: 3000 },
		);
		await waitFor(() => {
			expect(
				screen.getByText(/The scripted engine streamed this/),
			).toBeInTheDocument();
		});

		// Turn end drops the disposable query-cache draft. The two rendered
		// rows now come from the refetched authoritative page.
		expect(
			queryClient.getQueryData(
				turnDraftQueryKey(scriptedOqtoUiPlatform.id, SESSION),
			),
		).toBeNull();
		expect(screen.queryByText(i18n.t("oqtoUi.chat.pending"))).toBeNull();
		expect(screen.getAllByText("prove writable chat")).toHaveLength(1);
		expect(scriptedStore.get(SESSION)).toHaveLength(initialLength + 2);
	});

	it("renders durable mixed turns with the shared canonical anatomy", async () => {
		const sessionId = "frontend-rebuild";
		const originalTimeline = scriptedStore.get(sessionId) ?? [];
		scriptedStore.set(sessionId, [
			{
				id: "assistant-mixed",
				author: "agent",
				content: "Canonical final answer",
				time: "14:45",
				parts: [
					{ type: "thinking", text: "reasoning trace" },
					{
						type: "tool_call",
						toolCallId: "todo-1",
						name: "TodoWrite",
						input: {
							todos: [
								{
									content: "raw todo payload must stay hidden",
									status: "completed",
									priority: "high",
								},
							],
						},
						status: "success",
					},
					{
						type: "tool_result",
						toolCallId: "todo-1",
						name: "TodoWrite",
						output: '{"todos":"raw todo result must stay hidden"}',
						isError: false,
					},
					{ type: "text", text: "Canonical final answer" },
					{
						type: "file_ref",
						uri: "src/main.rs",
						label: "main.rs",
						range: { startLine: 7, endLine: 9 },
					},
				],
			},
		]);
		const queryClient = new QueryClient({
			defaultOptions: { queries: { retry: false } },
		});
		const onOpenFile = vi.fn();
		const { container, unmount } = render(
			<QueryClientProvider client={queryClient}>
				<ChatPane
					platform={scriptedOqtoUiPlatform}
					agentName="Pi"
					sessionId={sessionId}
					tasks={[]}
					workspacePath="/workspace"
					onOpenFile={onOpenFile}
				/>
			</QueryClientProvider>,
		);

		await screen.findByText("Canonical final answer");
		fireEvent.click(await screen.findByRole("button", { name: /main\.rs/ }));
		expect(onOpenFile).toHaveBeenCalledWith("src/main.rs", {
			startLine: 7,
			endLine: 9,
		});
		expect(container.querySelectorAll(".wb-message")).toHaveLength(0);
		expect(screen.queryByText(/raw todo payload/)).toBeNull();
		expect(screen.queryByText(/raw todo result/)).toBeNull();

		unmount();
		queryClient.clear();
		scriptedStore.set(sessionId, originalTimeline);
	});
});
