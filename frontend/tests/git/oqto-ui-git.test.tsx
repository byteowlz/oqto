import { GitPane, staged, unstaged } from "@/src/oqto-ui/git/GitPane";
import type {
	GitEntry,
	GitHost,
	GitStatus,
} from "@/src/oqto-ui/platform/git-contract";
import type { MuxSocket } from "@/src/oqto-ui/platform/mux-files";
import { createMuxGitHost } from "@/src/oqto-ui/platform/mux-git";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { initI18n } from "../../lib/i18n";

initI18n();

function entry(path: string, index: string, worktree: string): GitEntry {
	return { path, index, worktree, renamedFrom: null };
}

const ENTRIES = [
	entry("staged.ts", "M", " "),
	entry("dirty.ts", " ", "M"),
	entry("both.ts", "M", "M"),
	entry("new.ts", "?", "?"),
];

function status(overrides: Partial<GitStatus> = {}): GitStatus {
	return {
		branch: "main",
		upstream: "origin/main",
		ahead: 2,
		behind: 0,
		entries: ENTRIES,
		truncated: false,
		...overrides,
	};
}

function host(overrides: Partial<GitHost> = {}): GitHost {
	return {
		async status() {
			return status();
		},
		async diff(_workspacePath, path, isStaged) {
			return { path, staged: isStaged, patch: `--- ${path}`, truncated: false };
		},
		async log() {
			return [];
		},
		async stage() {},
		async commit() {
			return "abc123";
		},
		...overrides,
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

describe("git adapter", () => {
	it("maps the channel's status record, keeping both status codes", async () => {
		const { socket, sent } = fakeSocket();
		const git = createMuxGitHost(() => socket);
		const pending = git.status("/work/repo");
		socket.open();
		expect(sent[0]).toEqual({
			channel: "git",
			id: "git-1",
			type: "status",
			workspace_path: "/work/repo",
		});
		socket.deliver({
			channel: "git",
			type: "status_result",
			id: "git-1",
			branch: "main",
			upstream: "origin/main",
			ahead: 1,
			behind: 3,
			truncated: true,
			entries: [
				{ path: "a.ts", index: "M", worktree: " " },
				{ path: "b.ts", index: "R", worktree: " ", renamed_from: "old.ts" },
			],
		});
		const result = await pending;
		expect(result.branch).toBe("main");
		expect([result.ahead, result.behind]).toEqual([1, 3]);
		expect(result.truncated).toBe(true);
		expect(result.entries[1]).toEqual({
			path: "b.ts",
			index: "R",
			worktree: " ",
			renamedFrom: "old.ts",
		});
	});

	it("stages and unstages through the command each one names", async () => {
		const { socket, sent } = fakeSocket();
		const git = createMuxGitHost(() => socket);
		git.stage("/work/repo", ["a.ts"], true);
		socket.open();
		expect(sent[0]).toMatchObject({ type: "stage", paths: ["a.ts"] });
		git.stage("/work/repo", ["a.ts"], false);
		expect(sent[1]).toMatchObject({ type: "unstage", paths: ["a.ts"] });
	});

	it("reports commit timestamps in milliseconds", async () => {
		const { socket } = fakeSocket();
		const git = createMuxGitHost(() => socket);
		const pending = git.log("/work/repo", 2);
		socket.open();
		socket.deliver({
			channel: "git",
			type: "log_result",
			id: "git-1",
			commits: [
				{
					id: "abc",
					short_id: "ab",
					summary: "Fix",
					author: "Ada",
					timestamp: 1_700_000_000,
				},
			],
		});
		const commits = await pending;
		expect(commits[0].timestamp).toBe(1_700_000_000_000);
	});

	it("rejects the matching request on a channel error", async () => {
		const { socket } = fakeSocket();
		const git = createMuxGitHost(() => socket);
		const pending = git.status("/work/repo");
		socket.open();
		socket.deliver({
			channel: "git",
			type: "error",
			id: "git-1",
			error: "not a repository",
		});
		await expect(pending).rejects.toThrow("not a repository");
	});
});

describe("git pane", () => {
	it("splits a path changed on both sides into both lists", () => {
		expect(staged(ENTRIES).map((item) => item.path)).toEqual([
			"staged.ts",
			"both.ts",
		]);
		expect(unstaged(ENTRIES).map((item) => item.path)).toEqual([
			"dirty.ts",
			"both.ts",
			"new.ts",
		]);
	});

	it("shows the branch, its distance, and both change lists", async () => {
		render(<GitPane gitHost={host()} workspacePath="/work" />);
		expect(await screen.findByText("main")).toBeInTheDocument();
		expect(screen.getByText("2 ahead")).toBeInTheDocument();
		expect(screen.getByText("Staged")).toBeInTheDocument();
		expect(screen.getByText("Changed")).toBeInTheDocument();
	});

	it("stages one path and reloads the status", async () => {
		const stage = vi.fn(async () => {});
		const statusCalls = vi.fn(async () => status());
		render(
			<GitPane
				gitHost={host({ stage, status: statusCalls })}
				workspacePath="/work"
			/>,
		);
		await screen.findByText("main");
		fireEvent.click(screen.getByRole("button", { name: "Stage dirty.ts" }));
		await waitFor(() =>
			expect(stage).toHaveBeenCalledWith("/work", ["dirty.ts"], true),
		);
		// The listing is refreshed from the host, never patched locally.
		await waitFor(() => expect(statusCalls).toHaveBeenCalledTimes(2));
	});

	it("shows a path's diff when it is chosen", async () => {
		render(<GitPane gitHost={host()} workspacePath="/work" />);
		await screen.findByText("main");
		fireEvent.click(screen.getAllByText("dirty.ts")[0]);
		expect(await screen.findByText("--- dirty.ts")).toBeInTheDocument();
	});

	it("refuses to commit with nothing staged or no message", async () => {
		const commit = vi.fn(async () => "abc");
		render(
			<GitPane
				gitHost={host({ commit, status: async () => status({ entries: [] }) })}
				workspacePath="/work"
			/>,
		);
		await screen.findByText("main");
		const button = screen.getByRole("button", { name: /Commit/ });
		expect(button).toBeDisabled();
		expect(commit).not.toHaveBeenCalled();
	});

	it("commits the staged set with the typed message", async () => {
		const commit = vi.fn(async () => "abc123");
		render(<GitPane gitHost={host({ commit })} workspacePath="/work" />);
		await screen.findByText("main");
		fireEvent.change(screen.getByLabelText("Commit message"), {
			target: { value: "Fix the thing" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Commit 2 files" }));
		await waitFor(() =>
			expect(commit).toHaveBeenCalledWith("/work", "Fix the thing"),
		);
	});

	it("says what went wrong instead of showing an empty tree", async () => {
		render(
			<GitPane
				gitHost={host({
					status: async () => {
						throw new Error("not a git repository");
					},
				})}
				workspacePath="/work"
			/>,
		);
		expect(await screen.findByText("not a git repository")).toBeInTheDocument();
	});
});
