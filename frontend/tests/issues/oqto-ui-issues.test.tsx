import { IssuesPane } from "@/src/oqto-ui/issues/IssuesPane";
import type {
	Issue,
	IssueHost,
	IssueStatus,
} from "@/src/oqto-ui/platform/issues-contract";
import type { MuxSocket } from "@/src/oqto-ui/platform/mux-files";
import { createMuxIssueHost } from "@/src/oqto-ui/platform/mux-issues";
import { render, screen, waitFor } from "@testing-library/react";
import { fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { initI18n } from "../../lib/i18n";

initI18n();

function issue(id: string, status: IssueStatus, priority = 2): Issue {
	return {
		id,
		title: `title ${id}`,
		status,
		priority,
		kind: "task",
		parentId: null,
		blockedBy: status === "blocked" ? ["oqto-zz"] : [],
		labels: [],
	};
}

function fakeSocket() {
	const sent: Record<string, unknown>[] = [];
	const socket: MuxSocket & { open(): void; deliver(event: object): void } = {
		send: (data) => sent.push(JSON.parse(data) as Record<string, unknown>),
		close: () => socket.onclose?.call(null, null),
		onopen: null,
		onmessage: null,
		onclose: null,
		open: () => socket.onopen?.call(null, null),
		deliver: (event) =>
			socket.onmessage?.call(null, { data: JSON.stringify(event) }),
	};
	return { socket, sent };
}

describe("issues adapter", () => {
	it("lists over the trx channel and maps the wire record", async () => {
		const { socket, sent } = fakeSocket();
		const host = createMuxIssueHost(() => socket);
		const pending = host.list("/work/repo");
		socket.open();
		expect(sent[0]).toEqual({
			channel: "trx",
			id: "trx-1",
			type: "list",
			workspace_path: "/work/repo",
		});
		socket.deliver({
			channel: "trx",
			type: "list_result",
			id: "trx-1",
			issues: [
				{
					id: "oqto-1",
					title: "one",
					status: "in_progress",
					priority: 1,
					issue_type: "bug",
					blocked_by: ["oqto-2"],
					labels: ["ui"],
				},
				{ id: "oqto-2", title: "two", status: "nonsense" },
			],
		});
		const issues = await pending;
		expect(issues[0]).toEqual({
			id: "oqto-1",
			title: "one",
			status: "in_progress",
			priority: 1,
			kind: "bug",
			parentId: null,
			blockedBy: ["oqto-2"],
			labels: ["ui"],
		});
		// An unknown status falls back rather than reaching the pane as-is.
		expect(issues[1].status).toBe("open");
	});

	it("closes through the tracker's own close command, not a status update", async () => {
		const { socket, sent } = fakeSocket();
		const host = createMuxIssueHost(() => socket);
		host.setStatus("/work/repo", "oqto-1", "closed", "done");
		socket.open();
		expect(sent[0]).toEqual({
			channel: "trx",
			id: "trx-1",
			type: "close",
			workspace_path: "/work/repo",
			issue_id: "oqto-1",
			reason: "done",
		});
		host.setStatus("/work/repo", "oqto-1", "in_progress");
		expect(sent[1]).toMatchObject({
			type: "update",
			issue_id: "oqto-1",
			data: { status: "in_progress" },
		});
	});
});

describe("issues pane", () => {
	function host(overrides: Partial<IssueHost> = {}): IssueHost {
		return {
			async list() {
				return [
					issue("oqto-a", "open", 3),
					issue("oqto-b", "in_progress", 1),
					issue("oqto-c", "blocked", 0),
					issue("oqto-d", "closed"),
				];
			},
			async setStatus() {},
			...overrides,
		};
	}

	it("groups by status in tracker order and names what blocks an issue", async () => {
		render(<IssuesPane issueHost={host()} workspacePath="/work" />);
		await screen.findByText("title oqto-b");
		// Closed issues are opt-in: a real tracker is mostly closed issues.
		const headings = screen
			.getAllByRole("heading")
			.map((heading) => heading.textContent);
		expect(headings).toEqual(["In progress", "Blocked", "Open"]);
		expect(screen.queryByText("title oqto-d")).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Show closed issues" }));
		expect(screen.getByText("title oqto-d")).toBeInTheDocument();
		expect(screen.getByText("blocked by oqto-zz")).toBeInTheDocument();
	});

	it("starts an open issue and closes one in progress", async () => {
		const setStatus = vi.fn(async () => {});
		render(
			<IssuesPane issueHost={host({ setStatus })} workspacePath="/work" />,
		);
		await screen.findByText("title oqto-a");
		fireEvent.click(screen.getByRole("button", { name: "Start oqto-a" }));
		await waitFor(() =>
			expect(setStatus).toHaveBeenCalledWith("/work", "oqto-a", "in_progress"),
		);
		fireEvent.click(screen.getByRole("button", { name: "Close oqto-b" }));
		await waitFor(() =>
			expect(setStatus).toHaveBeenCalledWith("/work", "oqto-b", "closed"),
		);
		// A closed issue offers no further move.
		fireEvent.click(screen.getByRole("button", { name: "Show closed issues" }));
		expect(screen.queryByRole("button", { name: "Start oqto-d" })).toBeNull();
	});

	it("caps a group rather than rendering thousands of rows", async () => {
		const many = Array.from({ length: 130 }, (_, index) =>
			issue(`oqto-${index}`, "open"),
		);
		render(
			<IssuesPane
				issueHost={host({ list: async () => many })}
				workspacePath="/work"
			/>,
		);
		await screen.findByText("title oqto-0");
		expect(document.querySelectorAll(".wb-issues__group li").length).toBe(51);
		expect(screen.getByText("+80 more")).toBeInTheDocument();
	});

	it("says so when the tracker is unavailable", async () => {
		const failing = host({
			list: async () => {
				throw new Error("no trx");
			},
		});
		render(<IssuesPane issueHost={failing} workspacePath="/work" />);
		expect(
			await screen.findByText("The issue tracker is unavailable."),
		).toBeInTheDocument();
	});
});
