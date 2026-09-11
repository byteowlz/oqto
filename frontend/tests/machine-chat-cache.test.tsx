import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const readCachedChat = vi.fn();
const writeCachedChat = vi.fn();

vi.mock("@/lib/machine-chat-cache", () => ({
	machineChatKey: (account: string, machine: string, session: string) =>
		`${account} ${machine} ${session}`,
	readCachedChat: (key: string) => readCachedChat(key),
	writeCachedChat: (
		key: string,
		messages: unknown,
		hasMore: boolean,
		nextBefore?: string | null,
	) => writeCachedChat(key, messages, hasMore, nextBefore),
	clearCachedChats: vi.fn(),
}));

const { MachineConversation } = await import(
	"../src/routes/app-shell/MachineHistory"
);

afterEach(() => {
	cleanup();
	readCachedChat.mockReset();
	writeCachedChat.mockReset();
});

const session = {
	id: "oqto-native-1",
	title: "Saved Mac chat",
	workspace: "/Users/native/project",
	updated_at: "2026-09-09",
};

const storedMessage = {
	id: "m1",
	role: "assistant",
	parts: [{ id: "p1", part_type: "text", text: "answer from the machine" }],
};

function view(port: { call: ReturnType<typeof vi.fn> }) {
	return (
		<QueryClientProvider client={new QueryClient()}>
			<MachineConversation
				scope="account-1"
				label="Mac"
				session={session}
				port={port}
				close={() => {}}
			/>
		</QueryClientProvider>
	);
}

describe("machine chat caching", () => {
	it("keeps a conversation readable when the machine is unreachable", async () => {
		readCachedChat.mockResolvedValue({
			messages: [storedMessage],
			hasMore: false,
			nextBefore: null,
		});
		const port = {
			call: vi.fn().mockRejectedValue(new Error("machine offline")),
		};

		render(view(port));

		expect(await screen.findByText("answer from the machine")).toBeTruthy();
		expect(readCachedChat).toHaveBeenCalledWith(
			"account-1 /Users/native/project oqto-native-1",
		);
	});

	it("caches what it reads so the next open does not need the machine", async () => {
		const port = {
			call: vi.fn().mockResolvedValue({
				messages: [storedMessage],
				has_more: false,
			}),
		};

		render(view(port));

		expect(await screen.findByText("answer from the machine")).toBeTruthy();
		await waitFor(() =>
			expect(writeCachedChat).toHaveBeenCalledWith(
				"account-1 /Users/native/project oqto-native-1",
				[storedMessage],
				false,
				undefined,
			),
		);
	});

	it("surfaces the failure when nothing was ever cached", async () => {
		readCachedChat.mockResolvedValue(null);
		const port = {
			call: vi.fn().mockRejectedValue(new Error("machine offline")),
		};

		render(view(port));

		expect(await screen.findByRole("alert")).toBeTruthy();
	});
});
