import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
	type HistoryPort,
	MachineHistory,
} from "../src/routes/app-shell/MachineHistory";

afterEach(cleanup);
const session = {
	id: "oqto-native-1",
	title: "Saved Mac chat",
	workspace: "/Users/native/project",
	updated_at: "2026-09-09",
};
const page = {
	messages: [
		{
			id: "durable-message-1",
			role: "user",
			parts: [
				{
					id: "durable-part-1",
					part_type: "text",
					text: "Private saved text <script>never execute</script>",
				},
			],
		},
	],
	has_more: true,
	next_before: "opaque-cursor",
};
function shell(scope: string, port: HistoryPort, client = new QueryClient()) {
	return (
		<QueryClientProvider client={client}>
			<MachineHistory
				key={scope}
				scope={scope}
				label="Mac"
				close={vi.fn()}
				port={port}
			/>
		</QueryClientProvider>
	);
}
describe("read-only machine history", () => {
	it("reads the selected public ID and opaque page cursor, never sends or opens files", async () => {
		const call = vi
			.fn()
			.mockResolvedValueOnce({ sessions: [session] })
			.mockResolvedValueOnce(page)
			.mockResolvedValueOnce({ messages: [], has_more: false });
		render(shell("deployment:alice:mac", { call }));
		fireEvent.click(
			await screen.findByRole("button", { name: /Saved Mac chat/ }),
		);
		expect(await screen.findByText(/Private saved text/)).toBeVisible();
		expect(document.querySelector("script")).toBeNull();
		expect(document.querySelector("textarea")).toBeNull();
		expect(
			document.querySelector("[data-message-id='durable-message-1']"),
		).not.toBeNull();
		expect(call.mock.calls[1][0]).toEqual({
			command: "messages",
			session_id: session.id,
			before: null,
		});
		fireEvent.click(screen.getByRole("button", { name: "Older messages" }));
		await waitFor(() =>
			expect(call.mock.calls[2][0]).toEqual({
				command: "messages",
				session_id: session.id,
				before: "opaque-cursor",
			}),
		);
		expect(
			call.mock.calls.every(([command]) =>
				["list", "messages"].includes(command.command),
			),
		).toBe(true);
	});
	it("does not fall back to another machine when history is unavailable", async () => {
		const call = vi.fn().mockRejectedValue(new Error("offline"));
		render(shell("deployment:alice:mac", { call }));
		expect(await screen.findByRole("alert")).toHaveTextContent(
			"No other machine will be queried",
		);
		expect(call).toHaveBeenCalledTimes(1);
	});
	it("clears private history queries when the Account-scoped view unmounts", async () => {
		const client = new QueryClient();
		const call = vi.fn().mockResolvedValue({ sessions: [session] });
		const view = render(shell("deployment:alice:mac", { call }, client));
		await screen.findByRole("button", { name: /Saved Mac chat/ });
		view.unmount();
		await waitFor(() =>
			expect(
				client.getQueryCache().findAll({ queryKey: ["machine-history"] }),
			).toHaveLength(0),
		);
	});
});
