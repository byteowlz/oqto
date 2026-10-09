import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { i18n, initI18n } from "../lib/i18n";
import {
	SidebarSessions,
	type SidebarSessionsProps,
} from "../src/routes/app-shell/SidebarSessions";

vi.mock("../src/routes/app-shell/SidebarMachines", () => ({
	SidebarMachines: () => <div data-testid="remote-machines">Mac</div>,
}));
beforeAll(async () => {
	await initI18n();
	await i18n.changeLanguage("en");
});
afterEach(cleanup);

it("lists the hub first and additional machines below it", () => {
	const newChat = vi.fn();
	const props = {
		locale: "en",
		chatHistory: [],
		filteredSessions: [],
		sessionsByProject: [],
		sessionHierarchy: { parentSessions: [], childSessionsByParent: new Map() },
		selectedChatSessionId: null,
		busySessions: new Set(),
		runnerSessionCount: 0,
		expandedSessions: new Set(),
		expandedProjects: new Set(),
		pinnedSessions: new Set(),
		pinnedProjects: [],
		projectSortBy: "date",
		projectSortAsc: false,
		selectedProjectLabel: null,
		messageSearchExtraHits: [],
		onNewChat: newChat,
	} as unknown as SidebarSessionsProps;
	render(<SidebarSessions {...props} />);
	const heading = screen.getByRole("heading", { name: "Machines" });
	const search = screen.getByRole("textbox");
	const remote = screen.getByTestId("remote-machines");
	const local = screen.getByText("Hub");
	const before = (a: Element, b: Element) =>
		Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
	expect(before(search, heading)).toBe(true);
	expect(before(heading, local)).toBe(true);
	// Additional machines are peers listed under the hub, not above it.
	expect(before(local, remote)).toBe(true);
	expect(screen.queryByText("Sessions", { selector: "span" })).toBeNull();
	fireEvent.click(screen.getByTitle("New Session"));
	expect(newChat).toHaveBeenCalledOnce();
});
