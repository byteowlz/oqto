import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";
import { i18n, initI18n } from "../lib/i18n";
import { OqtoUiShell } from "../src/oqto-ui/app/OqtoUiShell";
import { scriptedOqtoUiPlatform } from "../src/oqto-ui/dev/scripted-platform";

initI18n();

function renderOqtoUi(entry = "/dev/oqto-ui") {
	initI18n();
	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	return render(
		<QueryClientProvider client={queryClient}>
			<MemoryRouter
				initialEntries={[entry]}
				future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
			>
				<OqtoUiShell platform={scriptedOqtoUiPlatform} mode="scripted" />
			</MemoryRouter>
		</QueryClientProvider>,
	);
}

afterEach(async () => {
	await act(async () => {
		await i18n.changeLanguage("en");
	});
});

describe("OqtoUI vertical slice", () => {
	it("settles the deterministic adapter to stable public identities", async () => {
		const snapshot = await scriptedOqtoUiPlatform.load("oqto-demo-review");
		expect(snapshot.activeSessionId).toBe("oqto-demo-review");
		expect(
			snapshot.sessions.every((session) => session.id.startsWith("oqto-")),
		).toBe(true);
		expect(snapshot.timeline.map((entry) => entry.id)).toEqual([
			"entry-user-1",
			"entry-agent-1",
			"entry-tool-1",
		]);
	});

	it("renders Sessions, real timeline semantics, and the bound Gallery App through one shell", async () => {
		renderOqtoUi();
		await waitFor(() =>
			expect(
				screen.getByRole("heading", { name: "Sessions" }),
			).toBeInTheDocument(),
		);
		expect(
			screen.getByRole("heading", { name: "Sessions" }),
		).toBeInTheDocument();
		expect(screen.getByRole("heading", { name: "Chat" })).toBeInTheDocument();
		expect(
			screen.getByRole("heading", { name: "Gallery App" }),
		).toBeInTheDocument();
		expect(screen.getByText("oqto.app.gallery")).toBeInTheDocument();
		expect(screen.getAllByRole("article")).toHaveLength(3);
		expect(
			screen.getByRole("link", { name: /Review image outputs/ }),
		).toHaveAttribute("href", "/dev/oqto-ui?session=oqto-demo-review");
	});

	it("ships matching German shell copy", async () => {
		await act(async () => {
			initI18n();
			await i18n.changeLanguage("de");
		});
		renderOqtoUi();
		await waitFor(() =>
			expect(
				screen.getByRole("heading", { name: "Sitzungen" }),
			).toBeInTheDocument(),
		);
		expect(screen.getByText("Agent-lokaler App-Kandidat")).toBeInTheDocument();
		expect(screen.getByText("Skript-Ablauf")).toBeInTheDocument();
	});
});
