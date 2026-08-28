import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ChatPane } from "../src/oqto-ui/chat/ChatPane";
import { turnDraftQueryKey } from "../src/oqto-ui/chat/query-keys";
import {
	scriptedOqtoUiPlatform,
	scriptedStore,
} from "../src/oqto-ui/dev/scripted-platform";

const SESSION = "chat-reliability";

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
				/>
			</QueryClientProvider>,
		);

		const composer = screen.getByRole("textbox");
		fireEvent.change(composer, { target: { value: "prove writable chat" } });
		fireEvent.click(screen.getByRole("button", { name: "oqtoUi.chat.send" }));

		expect(screen.getByText("prove writable chat")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "oqtoUi.chat.abort" }),
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
		expect(screen.queryByText("oqtoUi.chat.pending")).toBeNull();
		expect(screen.getAllByText("prove writable chat")).toHaveLength(1);
		expect(scriptedStore.get(SESSION)).toHaveLength(initialLength + 2);
	});
});
