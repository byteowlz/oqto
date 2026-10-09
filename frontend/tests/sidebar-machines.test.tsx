import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { i18n, initI18n } from "../lib/i18n";
import { SidebarMachines } from "../src/routes/app-shell/SidebarMachines";

const auth = vi.hoisted(() => ({
	user: { id: "alice" } as { id: string } | null,
}));
vi.mock("@/hooks/use-auth", () => ({
	useCurrentUser: () => ({ data: auth.user }),
}));
vi.mock("@/lib/control-plane-client", () => ({
	controlPlaneApiUrl: (path: string) => `https://control.example${path}`,
	getAuthHeaders: () => ({ Authorization: "Bearer fixture" }),
}));
const row = {
	id: "mac",
	label: "Mac",
	connection: "online",
	checked_at: "2026-09-08T10:00:00Z",
	session_creation: false,
	history_read: true,
};
beforeAll(async () => {
	await initI18n();
	await i18n.changeLanguage("en");
});
afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
	auth.user = { id: "alice" };
});

function shell(client: QueryClient) {
	return (
		<QueryClientProvider client={client}>
			<SidebarMachines />
		</QueryClientProvider>
	);
}

describe("original-shell machine inventory", () => {
	it("uses the configured control plane and authorization, not a localhost shortcut", async () => {
		const fetcher = vi
			.fn()
			.mockResolvedValue(new Response(JSON.stringify([row])));
		vi.stubGlobal("fetch", fetcher);
		render(shell(new QueryClient()));
		expect(await screen.findByText("Mac")).toBeVisible();
		// A machine is one sidebar row, not a status panel.
		expect(screen.queryByText("Online")).toBeNull();
		expect(screen.queryByText("Machines")).toBeNull();
		expect(document.querySelector(".wb-runner-targets")).toBeNull();
		expect(fetcher).toHaveBeenCalledWith(
			"https://control.example/api/runner-targets",
			{
				credentials: "include",
				headers: {
					Authorization: "Bearer fixture",
					Accept: "application/json",
				},
			},
		);
	});
	it("does not retain the previous Account's roster when Account identity changes", async () => {
		const fetcher = vi
			.fn()
			.mockImplementation(
				async () =>
					new Response(JSON.stringify(auth.user?.id === "alice" ? [row] : [])),
			);
		vi.stubGlobal("fetch", fetcher);
		const client = new QueryClient();
		const view = render(shell(client));
		await screen.findByText("Mac");
		auth.user = { id: "bob" };
		view.rerender(shell(client));
		await waitFor(() => expect(screen.queryByText("Mac")).toBeNull());
		expect(fetcher).toHaveBeenCalledTimes(2);
	});
	it("keeps provider login out of the row until the machine offers it", async () => {
		vi.stubGlobal(
			"fetch",
			vi.fn().mockResolvedValue(new Response(JSON.stringify([row]))),
		);
		render(shell(new QueryClient()));
		await screen.findByText("Mac");
		expect(screen.queryByRole("button", { name: /provider/i })).toBeNull();

		cleanup();
		vi.stubGlobal(
			"fetch",
			vi
				.fn()
				.mockResolvedValue(
					new Response(JSON.stringify([{ ...row, provider_login: true }])),
				),
		);
		render(shell(new QueryClient()));
		expect(
			await screen.findByRole("button", { name: /provider/i }),
		).toBeVisible();
	});
	it("withholds a roster it cannot verify", async () => {
		vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("offline")));
		render(shell(new QueryClient()));
		await waitFor(() => expect(screen.queryByText("Mac")).toBeNull());
	});
	it("does not probe before authentication", () => {
		auth.user = null;
		const fetcher = vi.fn();
		vi.stubGlobal("fetch", fetcher);
		render(shell(new QueryClient()));
		expect(fetcher).not.toHaveBeenCalled();
	});
});
