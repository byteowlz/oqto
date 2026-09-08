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
		expect(screen.getByText("Online")).toBeVisible();
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
	it("does not probe before authentication", () => {
		auth.user = null;
		const fetcher = vi.fn();
		vi.stubGlobal("fetch", fetcher);
		render(shell(new QueryClient()));
		expect(fetcher).not.toHaveBeenCalled();
	});
});
