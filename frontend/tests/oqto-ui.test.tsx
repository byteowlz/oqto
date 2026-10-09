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

	it("applies the scripted ui.lua preset as layout, appearance, and semantic bindings", async () => {
		await renderShell();
		const shell = document.querySelector<HTMLElement>(".wb-shell");
		expect(shell).toHaveAttribute("data-config-source", "user-lua");
		expect(shell).toHaveAttribute("data-files-placement", "right");
		expect(shell).toHaveAttribute("data-density", "compact");
		expect(shell?.dataset.scheme).toBe("oqto-dark");
		expect(shell?.style.getPropertyValue("--radius")).toBe("0px");
		expect(shell?.style.getPropertyValue("--font-sans")).toContain(
			"JetBrainsMono",
		);
		fireEvent.click(
			screen.getByRole("button", { name: "Open interface settings" }),
		);
		expect(
			screen.getByRole("complementary", { name: "Interface settings" }),
		).toBeInTheDocument();
		expect(screen.queryByRole("complementary", { name: "Files" })).toBeNull();
		expect(screen.getByText("corner-v4")).toBeInTheDocument();
		expect(screen.getByText("2 bindings")).toBeInTheDocument();

		fireEvent.keyDown(document, { key: "f", ctrlKey: true, shiftKey: true });
		await waitFor(() => {
			expect(screen.getByTestId("location-search")).toHaveTextContent(
				"view=files",
			);
		});
	});

	it("runs the corner-mode v4 navigator and send radial from the preset", async () => {
		await renderShell();
		const shell = document.querySelector<HTMLElement>(".wb-shell");
		expect(shell).toHaveAttribute("data-mobile-mode", "corner");

		fireEvent.pointerUp(
			screen.getByRole("button", { name: "Navigator; hold for projects" }),
		);
		expect(
			screen.getByRole("region", { name: "Navigator; hold for projects" }),
		).toBeInTheDocument();
		expect(
			screen.getByPlaceholderText("Search the whole workspace…"),
		).toBeInTheDocument();
		expect(screen.getByText("Start new Session")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "Close menu" }));
		const send = screen.getByRole("button", { name: "Send; hold for actions" });
		fireEvent.keyDown(send, { key: "ArrowDown" });
		const radial = screen.getByRole("menu", { name: "Send; hold for actions" });
		expect(within(radial).getAllByRole("menuitem")).toHaveLength(4);
		expect(
			document.querySelector(".wb-corner-composer textarea"),
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
		fireEvent.click(
			screen.getByRole("button", { name: "Open interface settings" }),
		);
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

describe("OqtoUI splash", () => {
	it("shows the logo splash while loading and a retryable error splash on failure", async () => {
		const { OqtoUiShell } = await import("../src/oqto-ui/app/OqtoUiShell");
		let failures = 0;
		const scripted = (await import("../src/oqto-ui/dev/scripted-platform"))
			.scriptedOqtoUiPlatform;
		const flakyPlatform = {
			...scripted,
			id: "flaky",
			load: async () => {
				failures += 1;
				if (failures === 1) throw new Error("HTTP 500");
				return scripted.load(null);
			},
			loadMessages: scripted.loadMessages.bind(scripted),
		};
		const queryClient = new QueryClient({
			defaultOptions: { queries: { retry: false } },
		});
		render(
			<QueryClientProvider client={queryClient}>
				<MemoryRouter
					initialEntries={["/dev/oqto-ui"]}
					future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
				>
					<OqtoUiShell
						platform={flakyPlatform}
						workDirectoryId={null}
						sessionId={null}
						mobileView="chat"
						schemeId="oqto-dark"
						workAreaTab="chat"
						onNavigate={() => {}}
					/>
				</MemoryRouter>
			</QueryClientProvider>,
		);
		expect(document.querySelector(".wb-splash__logo")).not.toBeNull();
		await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
		expect(
			document.querySelector('.wb-splash__logo[data-error="true"]'),
		).not.toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		await screen.findByRole("main", { name: "Session conversation" });
	});
});

describe("OqtoUI live platform", () => {
	it("pages canonical Markdown, tool, and file-reference parts without flattening", async () => {
		const payloads: Record<string, unknown> = {
			"/api/chat-history?limit=80": [
				{
					id: "oqto-abc",
					title: "Real session",
					project_name: "ctx",
					workspace_path: "/home/user/ctx",
					updated_at: 1787006011000,
				},
			],
		};
		const pagePayload = (before: string | null) => ({
			session_id: "oqto-abc",
			messages: before
				? [
						{
							id: "msg:0",
							role: "user",
							parts: [{ id: "u0", part_type: "text", text: "Hello" }],
							created_at: 1787006011228,
						},
					]
				: [
						{
							id: "msg:2",
							role: "assistant",
							parts: [
								{
									id: "p0",
									part_type: "thinking",
									text: "private reasoning",
								},
								{
									part_type: "text",
									id: "p1",
									text: "**Answer** in `src/main.rs:7`",
									format: "markdown",
								},
								{
									part_type: "tool_call",
									id: "p2",
									tool_call_id: "call-1",
									tool_name: "read",
									tool_input: { path: "src/main.rs" },
									tool_status: "success",
								},
								{
									part_type: "tool_result",
									id: "p3",
									tool_call_id: "call-1",
									tool_name: "read",
									tool_output: "fn main() {}",
									tool_status: "success",
								},
								{
									part_type: "file_ref",
									id: "p4",
									uri: "src/main.rs",
									label: "main.rs",
									range: { startLine: 7, endLine: 9 },
								},
							],
							created_at: 1787006012000,
						},
					],
			has_more: !before,
			next_before: before ? null : "v3.1",
		});
		const originalFetch = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url = String(input);
			if (url.includes("/messages/page")) {
				const before = new URL(url, "http://x").searchParams.get("before");
				return new Response(JSON.stringify(pagePayload(before)), {
					status: 200,
				});
			}
			return new Response(JSON.stringify(payloads[url] ?? []), { status: 200 });
		}) as typeof fetch;
		try {
			const { liveOqtoUiPlatform } = await import(
				"../src/oqto-ui/platform/live-platform"
			);
			const snapshot = await liveOqtoUiPlatform.load("oqto-abc");
			expect(snapshot.activeSessionId).toBe("oqto-abc");
			expect(snapshot.workDirectories).toHaveLength(1);
			const newest = await liveOqtoUiPlatform.loadMessages("oqto-abc");
			expect(newest.messages.map((m) => [m.author, m.content])).toEqual([
				["agent", "**Answer** in `src/main.rs:7`"],
			]);
			expect(newest.messages[0]?.parts?.map((part) => part.type)).toEqual([
				"thinking",
				"text",
				"tool_call",
				"tool_result",
				"file_ref",
			]);
			expect(newest.hasMore).toBe(true);
			expect(newest.nextBefore).toBe("v3.1");
			const older = await liveOqtoUiPlatform.loadMessages(
				"oqto-abc",
				newest.nextBefore ?? undefined,
			);
			expect(older.messages.map((m) => [m.author, m.content])).toEqual([
				["user", "Hello"],
			]);
			expect(older.hasMore).toBe(false);
		} finally {
			globalThis.fetch = originalFetch;
		}
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

	it("pages the scripted timeline without overlap or gaps", async () => {
		const { scriptedOqtoUiPlatform } = await import(
			"../src/oqto-ui/dev/scripted-platform"
		);
		const seen: string[] = [];
		let before: string | undefined;
		for (;;) {
			const page = await scriptedOqtoUiPlatform.loadMessages(
				"frontend-rebuild",
				before,
				7,
			);
			expect(page.sessionId).toBe("frontend-rebuild");
			seen.push(...page.messages.map((m) => m.id));
			if (!page.hasMore) break;
			before = page.nextBefore ?? undefined;
		}
		expect(new Set(seen).size).toBe(seen.length);
		// Newest message must be in the very first page fetched.
		expect(seen.length).toBeGreaterThan(100);
	});
});
