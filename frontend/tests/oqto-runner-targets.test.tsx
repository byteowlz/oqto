import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { i18n, initI18n } from "../lib/i18n";
import { parseRunnerTargets } from "../src/oqto-ui/platform/runner-targets";
import { RunnerTargets } from "../src/oqto-ui/sessions/RunnerTargets";

const row = {
	id: "mac",
	label: "Mac",
	connection: "online",
	checked_at: "2026-09-08T10:00:00Z",
	session_creation: false,
};

beforeAll(async () => {
	await initI18n();
	await i18n.changeLanguage("en");
});
afterEach(cleanup);

describe("runner target boundary", () => {
	it("accepts the backend contract and excludes transport details", () => {
		expect(parseRunnerTargets([{ ...row, endpoint: "secret" }])).toEqual([
			{
				id: "mac",
				label: "Mac",
				connection: "online",
				checkedAt: row.checked_at,
				sessionCreation: false,
			},
		]);
	});
	it("carries an explicit execution grant rather than assuming none", () => {
		const [granted] = parseRunnerTargets([{ ...row, session_creation: true }]);
		expect(granted.sessionCreation).toBe(true);
		expect(parseRunnerTargets([row])[0].sessionCreation).toBe(false);
	});
	it("rejects unsupported admission and malformed or duplicate targets", () => {
		for (const value of [
			null,
			[{}],
			[{ ...row, connection: "ready" }],
			[{ ...row, session_creation: "yes" }],
			[{ ...row, checked_at: "invalid" }],
			[row, row],
		]) {
			expect(() => parseRunnerTargets(value)).toThrow();
		}
	});
	it("shows connectivity, never a remote-chat launch action, and clears account caches", async () => {
		const client = new QueryClient({
			defaultOptions: { queries: { retry: false } },
		});
		const list = vi.fn().mockResolvedValue(parseRunnerTargets([row]));
		const view = render(
			<QueryClientProvider client={client}>
				<RunnerTargets source={{ id: "test", list }} />
			</QueryClientProvider>,
		);
		expect(await screen.findByText("Mac")).toBeVisible();
		expect(screen.getByText("Online")).toBeVisible();
		expect(screen.getByText("Chat not enabled")).toBeVisible();
		expect(screen.queryByRole("button")).toBeNull();
		view.unmount();
		await waitFor(() =>
			expect(
				client.getQueryData(["oqto-runner-targets", "test"]),
			).toBeUndefined(),
		);
	});
	it("withdraws the roster entirely when the control plane becomes unavailable", async () => {
		const client = new QueryClient();
		const list = vi.fn().mockResolvedValue(parseRunnerTargets([row]));
		render(
			<QueryClientProvider client={client}>
				<RunnerTargets source={{ id: "test", list }} />
			</QueryClientProvider>,
		);
		await screen.findByText("Online");
		list.mockRejectedValue(new Error("Disconnected"));
		await client.invalidateQueries({
			queryKey: ["oqto-runner-targets", "test"],
		});
		await waitFor(() => expect(screen.queryByText("Online")).toBeNull());
		// No stale status, and no error block left sitting in the sidebar.
		expect(screen.queryByText("Machine status unavailable")).toBeNull();
		expect(screen.queryByText("Mac")).toBeNull();
		expect(document.querySelector(".wb-runner-targets")).toBeNull();
	});
});
