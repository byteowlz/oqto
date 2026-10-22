import { i18n, initI18n } from "@/lib/i18n";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatTurnUpdate } from "../src/oqto-ui/platform/chat-contract";

const state = vi.hoisted(() => ({
	compact: false,
	count: 0,
	publish: (_update: ChatTurnUpdate) => {},
}));
vi.mock("@/hooks/use-mobile", () => ({ useIsMobile: () => state.compact }));
vi.mock("@tanstack/react-virtual", () => ({
	useVirtualizer: (options: {
		count: number;
		getItemKey(index: number): string;
	}) => {
		state.count = options.count;
		return {
			getTotalSize: () => options.count * 100,
			getVirtualItems: () =>
				Array.from({ length: options.count }, (_, index) => ({
					key: options.getItemKey(index),
					index,
					start: index * 100,
				})),
			measureElement: () => {},
		};
	},
}));
import { ChatPane } from "../src/oqto-ui/chat/ChatPane";

initI18n();
beforeEach(() => {
	state.compact = false;
	state.count = 0;
	vi.mocked(localStorage.getItem).mockReturnValue(null);
});
function mount() {
	const messages = Array.from({ length: 6 }, (_, index) => ({
		id: `history-${index}`,
		author: index % 2 ? ("agent" as const) : ("user" as const),
		content: `History row ${index}`,
		time: "12:00",
	}));
	const platform = {
		id: "transient-position-proof",
		loadMessages: vi.fn(async () => ({
			sessionId: "session",
			messages,
			hasMore: false,
			nextBefore: null,
		})),
		chat: {
			bind: (_id: string, publish: (update: ChatTurnUpdate) => void) => {
				state.publish = publish;
				return () => {};
			},
			send: vi.fn(() => "pending-native-request"),
			abort: vi.fn(),
		},
	};
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	render(
		<QueryClientProvider client={client}>
			<ChatPane
				platform={platform}
				agentName="Browser Agent"
				sessionId="session"
				tasks={[]}
				workspacePath="/workspace"
				onOpenFile={vi.fn()}
			/>
		</QueryClientProvider>,
	);
	return platform;
}
function positioned(text: string, index: number) {
	const row = screen
		.getByText(text, { exact: true })
		.closest<HTMLElement>(".wb-timeline-row");
	expect(row).not.toBeNull();
	expect(row?.dataset.index).toBe(String(index));
	expect(row?.style.getPropertyValue("--row-offset")).toBe(`${index * 100}px`);
}

describe("shared timeline transient row placement", () => {
	it("measures Working and the streaming reply after durable history, then removes it on end", async () => {
		mount();
		await screen.findByText("History row 5");
		await act(async () =>
			state.publish({ kind: "draft", draft: { parts: [] } }),
		);
		positioned(i18n.t("chat.working"), 6);
		expect(state.count).toBe(7);
		await act(async () =>
			state.publish({
				kind: "draft",
				draft: {
					parts: [
						{ type: "thinking", id: "thought", text: "Reasoning at the tail" },
						{
							type: "text",
							id: "answer",
							text: "Streaming answer at the tail",
						},
					],
				},
			}),
		);
		await screen.findByText("Streaming answer at the tail");
		positioned("Streaming answer at the tail", 6);
		expect(state.count).toBe(7);
		await act(async () => state.publish({ kind: "ended", reason: "error" }));
		await waitFor(() =>
			expect(screen.queryByText("Streaming answer at the tail")).toBeNull(),
		);
		expect(state.count).toBe(6);
	});

	it("includes the pending user prompt in desktop virtualizer measurements", async () => {
		const platform = mount();
		await screen.findByText("History row 5");
		fireEvent.change(
			screen.getByRole("textbox", { name: "Type a message..." }),
			{ target: { value: "Pending latest request" } },
		);
		fireEvent.click(
			screen.getByRole("button", { name: i18n.t("oqtoUi.chat.send") }),
		);
		expect(platform.chat.send).toHaveBeenCalled();
		positioned("Pending latest request", 6);
		expect(state.count).toBe(7);
	});

	it("keeps the same tail order in compact normal flow without absolute offsets", async () => {
		state.compact = true;
		mount();
		await screen.findByText("History row 5");
		await act(async () =>
			state.publish({ kind: "draft", draft: { parts: [] } }),
		);
		const row = screen
			.getByText(i18n.t("chat.working"))
			.closest<HTMLElement>(".wb-timeline-row");
		expect(row?.parentElement).toHaveClass("wb-timeline--plain");
		expect(row?.style.getPropertyValue("--row-offset")).toBe("");
		expect(row?.previousElementSibling).toHaveTextContent("History row 5");
		expect(state.count).toBe(0);
	});
});
