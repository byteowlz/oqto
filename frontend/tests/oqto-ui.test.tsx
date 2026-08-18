import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";
import { i18n, initI18n } from "../lib/i18n";
import DevOqtoUiRoute from "../src/oqto-ui/app/DevOqtoUiRoute";

initI18n();

function LocationProbe() {
	const location = useLocation();
	return <output data-testid="location-search">{location.search}</output>;
}

async function renderShell(initialEntry = "/dev/oqto-ui") {
	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	const view = render(
		<QueryClientProvider client={queryClient}>
			<MemoryRouter
				initialEntries={[initialEntry]}
				future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
			>
				<DevOqtoUiRoute />
				<LocationProbe />
			</MemoryRouter>
		</QueryClientProvider>,
	);
	await screen.findByRole("main", { name: "Session conversation" });
	return view;
}

afterEach(async () => {
	await act(async () => {
		await i18n.changeLanguage("en");
	});
});

describe("OqtoUI shell", () => {
	it("renders the session-centric desktop landmarks", async () => {
		await renderShell();
		expect(
			screen.getByRole("complementary", {
				name: "Workspace and session navigation",
			}),
		).toBeInTheDocument();
		expect(
			screen.getByRole("main", { name: "Session conversation" }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("complementary", { name: "Files" }),
		).toBeInTheDocument();
	});

	it("collapses mobile chrome into one bar with identity, session switch, and a tab menu", async () => {
		await renderShell();
		const bar = document.querySelector<HTMLElement>(".wb-mobile-chrome");
		const chrome = within(bar as HTMLElement);

		expect(bar?.querySelector(".wb-mobile-chrome__identity")).toHaveTextContent(
			"Frontend shell rebuild",
		);
		expect(bar?.querySelector(".wb-mobile-chrome__identity")).toHaveTextContent(
			"oqto_refactor [frontend-rebuild]",
		);
		expect(
			chrome.getByRole("button", { name: "Switch workspace or session" }),
		).toBeInTheDocument();

		const menu = chrome.getByRole("button", { name: "Destinations: Chat" });
		expect(menu).toHaveAttribute("aria-expanded", "false");
		fireEvent.click(menu);
		expect(menu).toHaveAttribute("aria-expanded", "true");

		const destinations = within(
			screen.getByRole("navigation", { name: "Destinations" }),
		);
		expect(
			destinations.getAllByRole("button").map((button) => button.textContent),
		).toEqual(["Chat", "Files", "OqtoUiShell.tsx", "Terminal", "Gallery"]);
	});

	it("opens session navigation from the mobile bar and closes it on selection", async () => {
		await renderShell();
		const shell = document.querySelector(".wb-shell");
		expect(shell).toHaveAttribute("data-sessions-open", "false");

		fireEvent.click(
			screen.getByRole("button", { name: "Switch workspace or session" }),
		);
		expect(shell).toHaveAttribute("data-sessions-open", "true");

		fireEvent.click(screen.getByText("Chat persistence diagnosis"));
		expect(shell).toHaveAttribute("data-sessions-open", "false");
	});

	it("keeps selected scope in the route search", async () => {
		await renderShell();
		fireEvent.click(screen.getByText("skillissues"));
		await waitFor(() => {
			expect(screen.getByTestId("location-search")).toHaveTextContent(
				"workDirectory=skillissues",
			);
		});
		const chatTab = await screen.findByRole("tab", {
			name: /Audit browser skills/,
		});
		expect(chatTab).toHaveAttribute(
			"title",
			expect.stringContaining("skillissues [skill-audit]"),
		);
		expect(screen.getByRole("button", { name: "Model" })).toHaveTextContent(
			"sonnet-4.6",
		);
	});

	it("owns work-area tab selection in the route with visible tab ownership", async () => {
		await renderShell(
			"/dev/oqto-ui?workDirectory=oqto&session=frontend-rebuild",
		);
		const workArea = within(
			screen.getByRole("tablist", { name: "Work area tabs" }),
		);
		const terminalTab = workArea.getByRole("tab", { name: /Terminal/ });
		expect(terminalTab).toHaveAttribute("title", "Work directory tab");
		fireEvent.click(terminalTab);
		await waitFor(() => {
			expect(screen.getByTestId("location-search")).toHaveTextContent(
				"tab=terminal",
			);
		});
		expect(screen.getByText("$ just check")).toBeInTheDocument();
	});

	it("opens the bound Gallery resources as a work-area tab", async () => {
		await renderShell();
		fireEvent.click(
			within(screen.getByRole("tablist", { name: "Work area tabs" })).getByRole(
				"tab",
				{ name: /Gallery/ },
			),
		);
		await waitFor(() => {
			expect(screen.getByTestId("location-search")).toHaveTextContent(
				"tab=gallery",
			);
		});
		const gallery = within(screen.getByRole("region", { name: "Gallery" }));
		expect(
			gallery.getByRole("button", { name: "Open cover.svg in Gallery" }),
		).toBeInTheDocument();
		expect(gallery.getAllByRole("listitem")).toHaveLength(4);
	});

	it("owns Base24 scheme selection in the route", async () => {
		await renderShell("/dev/oqto-ui?scheme=oqto-dark");
		fireEvent.click(screen.getByRole("button", { name: "Nord Light" }));
		await waitFor(() => {
			expect(screen.getByTestId("location-search")).toHaveTextContent(
				"scheme=nord-light",
			);
		});
		const shell = document.querySelector<HTMLElement>(".wb-shell");
		expect(shell?.dataset.scheme).toBe("nord-light");
		expect(shell?.style.getPropertyValue("--background")).not.toBe("");
	});

	it("ships matching German interface copy", async () => {
		await renderShell();
		await act(async () => {
			fireEvent.click(screen.getByRole("button", { name: "DE" }));
		});
		await waitFor(() => {
			expect(
				screen.getByRole("complementary", {
					name: "Arbeitsbereichs- und Sitzungsnavigation",
				}),
			).toBeInTheDocument();
		});
		fireEvent.click(screen.getByRole("button", { name: "Ziele: Chat" }));
		expect(
			within(screen.getByRole("navigation", { name: "Ziele" })).getByRole(
				"button",
				{ name: "Dateien" },
			),
		).toBeInTheDocument();
	});
});

describe("OqtoUI scripted platform", () => {
	it("resolves unknown session requests to the first scripted session", async () => {
		const { scriptedOqtoUiPlatform } = await import(
			"../src/oqto-ui/dev/scripted-platform"
		);
		const fallback = await scriptedOqtoUiPlatform.load("missing-session");
		expect(fallback.activeSessionId).toBe("frontend-rebuild");
		const explicit = await scriptedOqtoUiPlatform.load("skill-audit");
		expect(explicit.activeSessionId).toBe("skill-audit");
	});
});
