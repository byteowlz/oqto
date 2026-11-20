import { StatusBar } from "@/components/status-bar";
import { StatusBarLayout } from "@/components/status-bar-layout";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/components/contexts", () => ({
	useSelectedChat: () => ({ selectedChatFromHistory: null }),
}));
vi.mock("@/components/model-quick-switcher", () => ({
	ModelQuickSwitcher: () => <button type="button">Current model</button>,
}));
vi.mock("@/hooks/use-app", () => ({
	useApp: () => ({
		workspaceSessions: [],
		selectedChatSessionId: "fixture-session",
		runnerSessionCount: 3,
	}),
}));
vi.mock("@/hooks/use-auth", () => ({
	useCurrentUser: () => ({ data: { role: "admin" } }),
}));
vi.mock("@tanstack/react-query", () => ({
	useQuery: ({ queryKey }: { queryKey: string[] }) => ({
		data:
			queryKey[0] === "health"
				? { version: "0.5.11" }
				: {
						active_users: 1,
						total_users: 2,
						running_sessions: 3,
						total_sessions: 4,
					},
	}),
}));

const message =
	"Catalog refresh failed.\nThe retained Session list may be stale.";

function view(notice?: Parameters<typeof StatusBarLayout>[0]["notice"]) {
	return (
		<StatusBarLayout
			left={<span>Model · thinking</span>}
			right={<span>v0.5.11</span>}
			notice={notice}
		/>
	);
}

describe("fixed status-bar notices", () => {
	it("keeps default-shell metrics and model controls alongside catalog recovery", () => {
		const retry = vi.fn();
		render(
			<StatusBar
				notice={{
					title: "Chat history unavailable",
					message,
					recovery: { label: "Retry", onActivate: retry },
				}}
			/>,
		);
		expect(screen.getByRole("button", { name: "Current model" })).toBeVisible();
		expect(screen.getByText("3")).toBeVisible();
		expect(screen.getByText("1/2")).toBeVisible();
		expect(screen.getByText("3/4")).toBeVisible();
		expect(screen.getByText("v0.5.11")).toBeVisible();
		fireEvent.click(
			screen.getByRole("button", {
				name: `Chat history unavailable: ${message}`,
			}),
		);
		expect(retry).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		expect(retry).toHaveBeenCalledTimes(1);
	});

	it("keeps a permanent lane and announces full diagnostics without recovery side effects", () => {
		const retry = vi.fn();
		const { container, rerender } = render(view());
		const lane = screen.getByRole("status").parentElement;
		const bar = container.firstElementChild;
		expect(screen.getByRole("status")).toBeEmptyDOMElement();
		const notice = {
			title: "Chat history unavailable",
			message,
			recovery: { label: "Retry", onActivate: retry },
		};
		rerender(view(notice));
		expect(container.firstElementChild).toBe(bar);
		expect(screen.getByRole("status").parentElement).toBe(lane);
		expect(bar?.children).toHaveLength(3);
		expect(screen.getByRole("status").textContent).toBe(
			`${notice.title}: ${message}`,
		);
		expect(retry).not.toHaveBeenCalled();
		fireEvent.click(
			screen.getByRole("button", { name: `${notice.title}: ${message}` }),
		);
		expect(
			screen.getByRole("dialog", { name: notice.title }),
		).toHaveAccessibleDescription(message);
		expect(retry).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		expect(retry).toHaveBeenCalledTimes(1);
		rerender(view());
		expect(screen.queryByRole("dialog")).toBeNull();
		expect(screen.getByRole("status").parentElement).toBe(lane);
		expect(retry).toHaveBeenCalledTimes(1);
	});

	it("renders untrusted diagnostics as plain text and allows read-only notices", () => {
		const notice = {
			title: "Unavailable",
			message: "<img src=x onerror=alert(1)>",
		};
		const { container } = render(view(notice));
		fireEvent.click(
			screen.getByRole("button", {
				name: `${notice.title}: ${notice.message}`,
			}),
		);
		expect(screen.getByRole("dialog")).toHaveTextContent(notice.message);
		expect(container.querySelector("img")).toBeNull();
		expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
	});
});
