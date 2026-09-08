import { CompositorShell } from "@/src/oqto-ui/app/CompositorShell";
import DevOqtoUiRoute from "@/src/oqto-ui/app/DevOqtoUiRoute";
import { createPersistedCompositorStore } from "@/src/oqto-ui/compositor/react/persisted-store";
import { scriptedOqtoUiPlatform } from "@/src/oqto-ui/dev/scripted-platform";
import {
	type LayoutDocumentStore,
	layoutStorageKey,
	memoryLayoutStorage,
} from "@/src/oqto-ui/platform/layout-storage";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";
import { i18n, initI18n } from "../../lib/i18n";
import { classicLayout, containerByRole } from "./fixtures";

initI18n();

function renderShell(storage: LayoutDocumentStore, mobileView = "chat") {
	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	return render(
		<QueryClientProvider client={queryClient}>
			<MemoryRouter
				future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
			>
				<CompositorShell
					platform={scriptedOqtoUiPlatform}
					workDirectoryId={null}
					sessionId={null}
					mobileView={mobileView}
					schemeId={null}
					workAreaTab="chat"
					storage={storage}
					onNavigate={() => {}}
				/>
			</MemoryRouter>
		</QueryClientProvider>,
	);
}

afterEach(async () => {
	await act(async () => {
		await i18n.changeLanguage("en");
	});
});

describe("persisted compositor store", () => {
	it("recovers primary, promotes it to last-known-good, and writes after every accepted transaction", () => {
		const storage = memoryLayoutStorage();
		const first = createPersistedCompositorStore({
			storage,
			key: "k",
			fallback: classicLayout(),
		});
		expect(first.recovery.sourceIndex).toBeNull();
		first.commit([
			{
				type: "collapse",
				containerId: containerByRole(first.getSnapshot(), "navigation").id,
				collapsed: true,
			},
		]);
		expect(storage.read("k")).not.toBeNull();
		const second = createPersistedCompositorStore({
			storage,
			key: "k",
			fallback: classicLayout(),
		});
		expect(second.recovery.sourceIndex).toBe(0);
		expect(containerByRole(second.getSnapshot(), "navigation").collapsed).toBe(
			true,
		);
		expect(second.getSnapshot().revision).toBe(1);
		expect(storage.read("k:last-known-good")).toBe(storage.read("k"));
	});

	it("falls back to last-known-good when the primary document is corrupt", () => {
		const storage = memoryLayoutStorage();
		const first = createPersistedCompositorStore({
			storage,
			key: "k",
			fallback: classicLayout(),
		});
		first.commit([
			{
				type: "collapse",
				containerId: containerByRole(first.getSnapshot(), "navigation").id,
				collapsed: true,
			},
		]);
		createPersistedCompositorStore({
			storage,
			key: "k",
			fallback: classicLayout(),
		});
		storage.write("k", "{corrupt");
		const third = createPersistedCompositorStore({
			storage,
			key: "k",
			fallback: classicLayout(),
		});
		expect(third.recovery.sourceIndex).toBe(1);
		expect(third.recovery.failures[0]?.reason).toBe("parse-error");
		expect(containerByRole(third.getSnapshot(), "navigation").collapsed).toBe(
			true,
		);
	});
});

describe("CompositorShell with the real panes", () => {
	it("renders navigation, chat, and files as Container Content inside compositor cells", async () => {
		const view = renderShell(memoryLayoutStorage());
		await screen.findByRole("main", { name: "Session conversation" });
		expect(
			screen.getByRole("complementary", {
				name: "Workspace and session navigation",
			}),
		).toBeInTheDocument();
		expect(
			screen.getByRole("complementary", { name: "Files" }),
		).toBeInTheDocument();
		const cells = view.container.querySelectorAll(".oqto-compositor-cell");
		expect(cells.length).toBe(4);
		// The sidebar is a full-height inline-start edge Container (flush
		// top/left/bottom); the status row sits under the content column only.
		const navigationCell = view.container.querySelector(
			'.oqto-compositor-cell:has([data-role="navigation"])',
		);
		expect(navigationCell).toHaveAttribute("data-full-height");
		expect(navigationCell?.getAttribute("data-edges")).toBe(
			"inline-start block-start block-end",
		);
		const statusCell = view.container.querySelector(
			'.oqto-compositor-cell:has([data-role="status"])',
		);
		expect(statusCell?.getAttribute("data-edges")).toBe("inline-end block-end");
		expect(
			view.container.querySelector(".oqto-compositor-grid"),
		).not.toBeNull();
		expect(
			view.container.querySelector('[data-role="primary"] main'),
		).not.toBeNull();
	});

	it("restores a persisted Arrangement across remounts", async () => {
		const storage = memoryLayoutStorage();
		const key = layoutStorageKey(
			scriptedOqtoUiPlatform.id,
			"desktop-classic-3",
		);
		const seed = createPersistedCompositorStore({
			storage,
			key,
			fallback: classicLayout(),
		});
		seed.commit([
			{
				type: "collapse",
				containerId: containerByRole(seed.getSnapshot(), "navigation").id,
				collapsed: true,
			},
		]);
		const view = renderShell(storage);
		await screen.findByRole("main", { name: "Session conversation" });
		const navigation = view.container.querySelector('[data-role="navigation"]');
		expect(navigation).toHaveAttribute("hidden");
		// The current Session's Chat was opened idempotently into primary.
		expect(storage.read(key)).toContain("chat:");
	});
});

describe("dev route compositor switch", () => {
	it("composes the scripted platform through the compositor with ?compositor=1", async () => {
		const queryClient = new QueryClient({
			defaultOptions: { queries: { retry: false } },
		});
		const view = render(
			<QueryClientProvider client={queryClient}>
				<MemoryRouter
					initialEntries={["/dev/oqto-ui?compositor=1"]}
					future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
				>
					<DevOqtoUiRoute />
				</MemoryRouter>
			</QueryClientProvider>,
		);
		await screen.findByRole("main", { name: "Session conversation" });
		expect(
			view.container.querySelector('[data-compositor="classic"]'),
		).not.toBeNull();
	});
});

function setViewportWidth(width: number) {
	Object.defineProperty(window, "innerWidth", {
		configurable: true,
		value: width,
	});
	window.dispatchEvent(new Event("resize"));
}

describe("OG shell parity chrome", () => {
	afterEach(() => setViewportWidth(1024));

	it("renders the status bar and opens Settings as auxiliary Content on desktop", async () => {
		setViewportWidth(1600);
		const view = renderShell(memoryLayoutStorage());
		await screen.findByRole("main", { name: "Session conversation" });
		// The status bar is preset Content in its own row, not chrome below the grid.
		expect(
			view.container.querySelector('[data-role="status"] .wb-statusbar'),
		).not.toBeNull();
		expect(
			view.container.querySelectorAll(".oqto-compositor-cell"),
		).toHaveLength(4);
		fireEvent.click(
			screen.getByRole("button", { name: "Open interface settings" }),
		);
		expect(
			screen.getByRole("tab", { name: "Interface settings" }),
		).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", { name: "Close interface settings" }),
		);
		expect(
			screen.queryByRole("tab", { name: "Interface settings" }),
		).toBeNull();
	});

	it("projects into one destination on mobile with a full-screen navigation drawer", async () => {
		setViewportWidth(600);
		const view = renderShell(memoryLayoutStorage());
		await screen.findByRole("main", { name: "Session conversation" });
		const shell = view.container.querySelector(".wb-shell") as HTMLElement;
		expect(shell).toHaveAttribute("data-compositor", "mobile");
		expect(view.container.querySelector(".oqto-compositor-grid")).toBeNull();
		expect(view.container.querySelector(".wb-workarea")).toHaveAttribute(
			"data-view",
			"chat",
		);
		expect(view.container.querySelector(".wb-mobile-chrome")).not.toBeNull();
		expect(
			screen.getByRole("complementary", { name: "Files" }),
		).toBeInTheDocument();
		expect(shell).toHaveAttribute("data-sessions-open", "false");
		fireEvent.click(
			screen.getByRole("button", { name: "Switch workspace or session" }),
		);
		expect(shell).toHaveAttribute("data-sessions-open", "true");
		expect(
			screen.getByRole("complementary", {
				name: "Workspace and session navigation",
			}),
		).toBeInTheDocument();
		fireEvent.click(screen.getByText("Chat persistence diagnosis"));
		expect(shell).toHaveAttribute("data-sessions-open", "false");
	});

	it("shows the Files destination on mobile when the view is files", async () => {
		setViewportWidth(600);
		const view = renderShell(memoryLayoutStorage(), "files");
		await screen.findByRole("complementary", { name: "Files" });
		expect(view.container.querySelector(".wb-workarea")).toHaveAttribute(
			"data-view",
			"files",
		);
	});
});

describe("collapse controls", () => {
	it("closes the mobile drawer without collapsing the desktop Container", async () => {
		setViewportWidth(600);
		const view = renderShell(memoryLayoutStorage());
		await screen.findByRole("main", { name: "Session conversation" });
		const shell = view.container.querySelector(".wb-shell") as HTMLElement;
		fireEvent.click(
			screen.getByRole("button", { name: "Switch workspace or session" }),
		);
		expect(shell).toHaveAttribute("data-sessions-open", "true");
		fireEvent.click(
			screen.getByRole("button", { name: "Collapse navigation" }),
		);
		expect(shell).toHaveAttribute("data-sessions-open", "false");
		// The drawer is view state: the sidebar must still be there on desktop.
		act(() => setViewportWidth(1600));
		expect(
			view.container.querySelector('[data-role="navigation"]'),
		).not.toHaveAttribute("hidden");
		expect(screen.queryByRole("button", { name: "Expand sidebar" })).toBeNull();
	});

	it("gives the collapsed sidebar a visible way back", async () => {
		setViewportWidth(1600);
		const view = renderShell(memoryLayoutStorage());
		await screen.findByRole("main", { name: "Session conversation" });
		fireEvent.click(
			screen.getByRole("button", { name: "Collapse navigation" }),
		);
		const rail = screen.getByRole("button", { name: "Expand sidebar" });
		expect(rail).toHaveAttribute("title", "Expand sidebar");
		expect(rail.querySelector("svg")).not.toBeNull();
		expect(
			view.container.querySelector('[data-role="navigation"]'),
		).toHaveAttribute("hidden");
	});

	it("collapses the sidebar from its button, re-expands from the edge strip, and toggles the right Container", async () => {
		setViewportWidth(1600);
		const view = renderShell(memoryLayoutStorage());
		await screen.findByRole("main", { name: "Session conversation" });
		fireEvent.click(
			screen.getByRole("button", { name: "Collapse navigation" }),
		);
		const navigation = view.container.querySelector('[data-role="navigation"]');
		expect(navigation).toHaveAttribute("hidden");
		fireEvent.click(screen.getByRole("button", { name: "Expand sidebar" }));
		expect(navigation).not.toHaveAttribute("hidden");
		fireEvent.click(screen.getByRole("button", { name: "Toggle side panel" }));
		const auxiliary = view.container.querySelector('[data-role="auxiliary"]');
		expect(auxiliary).toHaveAttribute("hidden");
		fireEvent.click(screen.getByRole("button", { name: "Toggle side panel" }));
		expect(auxiliary).not.toHaveAttribute("hidden");
	});
});
