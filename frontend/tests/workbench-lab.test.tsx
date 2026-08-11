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
import WorkbenchLabRoute from "../src/workbench/routes/WorkbenchLabRoute";

function LocationProbe() {
	const location = useLocation();
	return <output data-testid="location-search">{location.search}</output>;
}

function renderLab(initialEntry = "/workbench-lab") {
	initI18n();
	return render(
		<MemoryRouter
			initialEntries={[initialEntry]}
			future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
		>
			<WorkbenchLabRoute />
			<LocationProbe />
		</MemoryRouter>,
	);
}

afterEach(async () => {
	await act(async () => {
		await i18n.changeLanguage("en");
	});
});

describe("Workbench Lab", () => {
	it("renders the session-centric desktop landmarks", () => {
		renderLab();
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
		expect(
			screen.queryByRole("tablist", { name: "Workspace tools" }),
		).not.toBeInTheDocument();
	});

	it("collapses mobile chrome into one bar with identity, session switch, and a tab menu", () => {
		renderLab();
		const bar = document.querySelector<HTMLElement>(".wb-mobile-chrome");
		const chrome = within(bar as HTMLElement);

		expect(bar?.querySelector(".wb-mobile-chrome__identity")).toHaveTextContent(
			"Frontend workbench rebuild",
		);
		expect(bar?.querySelector(".wb-mobile-chrome__identity")).toHaveTextContent(
			"oqto_refactor [frontend-rebuild]",
		);
		expect(
			chrome.getByRole("button", { name: "Switch workspace or session" }),
		).toBeInTheDocument();

		expect(
			screen.queryByRole("navigation", { name: "Workbench destinations" }),
		).not.toBeInTheDocument();

		const menu = chrome.getByRole("button", {
			name: "Workbench destinations: Chat",
		});
		expect(menu).toHaveAttribute("aria-expanded", "false");
		fireEvent.click(menu);
		expect(menu).toHaveAttribute("aria-expanded", "true");

		const destinations = within(
			screen.getByRole("navigation", { name: "Workbench destinations" }),
		);
		expect(
			destinations.getAllByRole("button").map((button) => button.textContent),
		).toEqual(["Chat", "Files", "WorkbenchShell.tsx", "Terminal"]);
	});

	it("opens session navigation from the mobile bar and closes it on selection", () => {
		renderLab();
		const shell = document.querySelector(".wb-shell");
		expect(shell).toHaveAttribute("data-sessions-open", "false");

		fireEvent.click(
			screen.getByRole("button", { name: "Switch workspace or session" }),
		);
		expect(shell).toHaveAttribute("data-sessions-open", "true");

		fireEvent.click(screen.getByText("Chat persistence diagnosis"));
		expect(shell).toHaveAttribute("data-sessions-open", "false");
	});

	it("projects the selected Session plan as a compact connected progress rail", () => {
		renderLab();
		const desktopRail = document.querySelector<HTMLElement>(
			'.wb-task-progress[data-placement="desktop"]',
		);
		expect(desktopRail).not.toBeNull();
		expect(desktopRail?.closest(".wb-chat-card")).not.toBeNull();
		expect(desktopRail?.closest(".wb-panel")).toBeNull();
		const progress = within(desktopRail as HTMLElement);
		expect(progress.getByText("2/5")).toBeInTheDocument();
		expect(
			desktopRail?.querySelector(".wb-task-progress__active"),
		).toHaveTextContent("Build the responsive surfaces");

		fireEvent.click(
			progress.getByRole("button", {
				name: "Build the responsive surfaces: In progress",
			}),
		);
		expect(progress.getByRole("region", { name: "Tasks" })).toBeInTheDocument();
		expect(progress.getByText("2 of 5 tasks completed")).toBeInTheDocument();
		expect(document.querySelector(".wb-tasks")).toBeNull();

		const mobileRail = document.querySelector<HTMLElement>(
			'.wb-task-progress[data-placement="mobile"]',
		);
		const mobileProgress = within(mobileRail as HTMLElement);
		fireEvent.click(
			mobileProgress.getByRole("button", {
				name: "Build the responsive surfaces: In progress",
			}),
		);
		expect(
			mobileProgress.getByRole("region", { name: "Tasks" }),
		).toBeInTheDocument();
	});

	it("keeps selected scope in the route search", async () => {
		renderLab();
		fireEvent.click(screen.getByText("skillissues"));
		await waitFor(() => {
			expect(screen.getByTestId("location-search")).toHaveTextContent(
				"workDirectory=skillissues",
			);
		});
		const chatTab = screen.getByRole("tab", { name: /Audit browser skills/ });
		expect(chatTab).toHaveAttribute(
			"title",
			expect.stringContaining("skillissues [skill-audit]"),
		);
		expect(screen.getByRole("button", { name: "Model" })).toHaveTextContent(
			"sonnet-4.6",
		);
		expect(
			screen.getByRole("button", { name: "Context window usage" }),
		).toHaveTextContent("9.8k · 5%");
		expect(
			document.querySelector('.wb-task-progress[data-placement="desktop"]'),
		).toBeNull();
	});

	it("routes between mobile destinations without discarding session scope", async () => {
		renderLab(
			"/workbench-lab?workDirectory=oqto&session=frontend-rebuild&view=chat",
		);
		fireEvent.click(
			screen.getByRole("button", { name: "Workbench destinations: Chat" }),
		);
		fireEvent.click(
			within(
				screen.getByRole("navigation", { name: "Workbench destinations" }),
			).getByRole("button", { name: "Files" }),
		);
		await waitFor(() => {
			expect(screen.getByTestId("location-search")).toHaveTextContent(
				"view=files",
			);
		});
		expect(screen.getByTestId("location-search")).toHaveTextContent(
			"session=frontend-rebuild",
		);
	});

	it("owns work-area tab selection in the route with visible tab ownership", async () => {
		renderLab("/workbench-lab?workDirectory=oqto&session=frontend-rebuild");
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
		expect(
			workArea.getByRole("tab", { name: /Frontend workbench rebuild/ }),
		).toHaveAttribute(
			"title",
			expect.stringContaining("oqto_refactor [frontend-rebuild]"),
		);
	});

	it("owns Base24 scheme selection in the route", async () => {
		renderLab("/workbench-lab?scheme=oqto-dark");
		fireEvent.click(screen.getByRole("button", { name: "Nord Light" }));
		await waitFor(() => {
			expect(screen.getByTestId("location-search")).toHaveTextContent(
				"scheme=nord-light",
			);
		});
		const shell = document.querySelector<HTMLElement>(".wb-shell");
		expect(shell?.dataset.scheme).toBe("nord-light");
		expect(shell?.style.getPropertyValue("--background")).not.toBe("");
		expect(screen.getByRole("button", { name: "Nord Light" })).toHaveAttribute(
			"aria-pressed",
			"true",
		);
	});

	it("lets the user override theme tokens as data and round-trips the JSON", () => {
		renderLab();
		const shell = document.querySelector<HTMLElement>(".wb-shell");
		expect(shell).toHaveAttribute("data-theme-source", "scheme");

		fireEvent.click(screen.getByRole("button", { name: "Customize theme" }));
		fireEvent.change(screen.getByLabelText("Primary"), {
			target: { value: "#ff0055" },
		});
		expect(shell?.style.getPropertyValue("--primary")).toBe("#ff0055");
		expect(shell).toHaveAttribute("data-theme-source", "user");
		expect(
			(screen.getByLabelText("User theme JSON") as HTMLTextAreaElement).value,
		).toContain('"--primary": "#ff0055"');

		fireEvent.change(screen.getByLabelText("Corner radius"), {
			target: { value: "12" },
		});
		expect(shell?.style.getPropertyValue("--radius")).toBe("12px");
		expect(shell?.style.getPropertyValue("--radius-sm")).toBe(
			"calc(12px * 0.5)",
		);

		fireEvent.change(screen.getByLabelText("Shadows"), {
			target: { value: "soft" },
		});
		expect(shell?.style.getPropertyValue("--shadow-md")).toContain("4px 12px");
		fireEvent.change(screen.getByLabelText("Backdrop blur"), {
			target: { value: "8" },
		});
		expect(shell?.style.getPropertyValue("--backdrop-blur")).toBe("8px");

		fireEvent.change(screen.getByLabelText("Interface font"), {
			target: { value: "mono" },
		});
		expect(shell?.style.getPropertyValue("--font-sans")).toContain(
			"JetBrainsMono Nerd Font",
		);
		expect(
			(screen.getByLabelText("User theme JSON") as HTMLTextAreaElement).value,
		).toContain('"fontSans"');
		fireEvent.change(screen.getByLabelText("Interface font"), {
			target: { value: "system" },
		});
		expect(shell?.style.getPropertyValue("--font-sans")).not.toContain(
			"JetBrainsMono Nerd Font",
		);

		fireEvent.change(screen.getByLabelText("User theme JSON"), {
			target: {
				value: JSON.stringify({ overrides: { "--background": "#112233" } }),
			},
		});
		fireEvent.click(screen.getByRole("button", { name: "Apply JSON theme" }));
		expect(shell?.style.getPropertyValue("--background")).toBe("#112233");
		expect(shell?.style.getPropertyValue("--primary")).not.toBe("#ff0055");

		fireEvent.change(screen.getByLabelText("User theme JSON"), {
			target: { value: "{ not json" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Apply JSON theme" }));
		expect(screen.getByRole("alert")).toBeInTheDocument();
		expect(shell?.style.getPropertyValue("--background")).toBe("#112233");

		fireEvent.click(
			screen.getByRole("button", { name: "Reset to scheme defaults" }),
		);
		expect(shell?.style.getPropertyValue("--background")).not.toBe("#112233");
		expect(shell).toHaveAttribute("data-theme-source", "scheme");
	});

	it("selects the session model from the composer chip", () => {
		renderLab();
		const chip = screen.getByRole("button", { name: "Model" });
		expect(chip).toHaveTextContent("opus-4.7");
		expect(chip.closest(".wb-statusbar")).not.toBeNull();

		fireEvent.click(chip);
		const menu = document.querySelector<HTMLElement>(
			".wb-composer__model-menu",
		);
		expect(
			within(menu as HTMLElement).getByRole("button", { name: "opus-4.7" }),
		).toHaveAttribute("aria-pressed", "true");
		fireEvent.click(
			within(menu as HTMLElement).getByRole("button", { name: "haiku-4.5" }),
		);
		expect(chip).toHaveTextContent("haiku-4.5");
		expect(document.querySelector(".wb-composer__model-menu")).toBeNull();

		const details = document.querySelector(".wb-composer__context-details");
		expect(details).toHaveTextContent("haiku-4.5");
		expect(details).toHaveTextContent("200k");
		expect(details).toHaveTextContent("24.1k · 12%");
	});

	it("ships matching German interface copy", async () => {
		renderLab();
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
		fireEvent.click(
			screen.getByRole("button", { name: "Workbench-Ziele: Chat" }),
		);
		expect(
			within(
				screen.getByRole("navigation", { name: "Workbench-Ziele" }),
			).getByRole("button", { name: "Dateien" }),
		).toBeInTheDocument();
	});
});
