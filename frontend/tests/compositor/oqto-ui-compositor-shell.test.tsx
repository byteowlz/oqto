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
import { act, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it } from "vitest";
import { i18n, initI18n } from "../../lib/i18n";
import { classicLayout, containerByRole } from "./fixtures";

initI18n();

function renderShell(storage: LayoutDocumentStore) {
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
		expect(cells.length).toBe(3);
		expect(
			view.container.querySelector(".oqto-compositor-grid"),
		).not.toBeNull();
		expect(
			view.container.querySelector('[data-role="primary"] main'),
		).not.toBeNull();
	});

	it("restores a persisted Arrangement across remounts", async () => {
		const storage = memoryLayoutStorage();
		const key = layoutStorageKey(scriptedOqtoUiPlatform.id, "desktop");
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
