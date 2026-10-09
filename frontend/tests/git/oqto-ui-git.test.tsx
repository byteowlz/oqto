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

/** A real unified patch: the renderer parses it, so a sketch will not do. */
const PATCH = [
	"diff --git a/dirty.ts b/dirty.ts",
	"index 1111111..2222222 100644",
	"--- a/dirty.ts",
	"+++ b/dirty.ts",
	"@@ -1,3 +1,3 @@",
	" const kept = 1;",
	"-const before = 2;",
	"+const after = 2;",
	" const also = 3;",
	"",
].join("\n");

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
			return { path, staged: isStaged, patch: PATCH, truncated: false };
		},
		async log() {
			return [];
		},
		async stage() {},
		async commit() {
			return "abc123";
		},
		branches: {
			async list() {
				return [
					{ name: "main", current: true, worktree: "/work" },
					{ name: "spike", current: false, worktree: null },
					{ name: "held", current: false, worktree: "/other" },
				];
			},
			async switch() {},
		},
		async remote(_workspacePath, operation) {
			return { operation, summary: `${operation} done` };
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

	it("lists branches and switches through their own commands", async () => {
		const { socket, sent } = fakeSocket();
		const git = createMuxGitHost(() => socket);
		const pending = git.branches.list("/work/repo");
		socket.open();
		expect(sent[0]).toMatchObject({ type: "branches" });
		socket.deliver({
			channel: "git",
			type: "branches_result",
			id: "git-1",
			current: "main",
			branches: [
				{ name: "main", current: true, worktree: "/work/repo" },
				{ name: "spike", current: false },
			],
		});
		const branches = await pending;
		expect(branches[1]).toEqual({
			name: "spike",
			current: false,
			worktree: null,
		});
		git.branches.switch("/work/repo", "spike");
		expect(sent[1]).toMatchObject({ type: "switch", branch: "spike" });
	});

	it("names the remote operation it is running", async () => {
		const { socket, sent } = fakeSocket();
		const git = createMuxGitHost(() => socket);
		const pending = git.remote("/work/repo", "pull");
		socket.open();
		expect(sent[0]).toMatchObject({ type: "pull" });
		socket.deliver({
			channel: "git",
			type: "remote_result",
			id: "git-1",
			operation: "pull",
			summary: "Already up to date.",
		});
		await expect(pending).resolves.toEqual({
			operation: "pull",
			summary: "Already up to date.",
		});
	});

	it("fails in flight when the host has no git channel at all", async () => {
		const { socket } = fakeSocket();
		const git = createMuxGitHost(() => socket);
		const pending = git.status("/work/repo");
		socket.open();
		// An older host answers an unknown channel on `system`; without this the
		// request would wait for a reply that is never coming.
		socket.deliver({
			channel: "system",
			type: "error",
			error: "Invalid command: unknown variant `git`",
		});
		await expect(pending).rejects.toThrow("unknown variant `git`");
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
		render(
			<GitPane gitHost={host()} workspacePath="/work" schemeId="oqto-dark" />,
		);
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
				schemeId="oqto-dark"
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

	it("hands a chosen path's patch to the diff renderer", async () => {
		const view = render(
			<GitPane gitHost={host()} workspacePath="/work" schemeId="oqto-dark" />,
		);
		await screen.findByText("main");
		fireEvent.click(screen.getAllByText("dirty.ts")[0]);
		// The renderer owns its own markup, inside a shadow root: what the pane
		// must prove is that the patch was parsed and rendered as a diff, not
		// printed as text.
		const surface = await waitFor(() => {
			const found = view.container.querySelector("diffs-container");
			expect(found).not.toBeNull();
			return found as HTMLElement;
		});
		await waitFor(() =>
			expect(surface.shadowRoot?.innerHTML ?? "").toContain("dirty.ts"),
		);
		expect(surface.shadowRoot?.innerHTML).toContain("data-deletions-count");
	});

	it("refuses to commit with nothing staged or no message", async () => {
		const commit = vi.fn(async () => "abc");
		render(
			<GitPane
				gitHost={host({ commit, status: async () => status({ entries: [] }) })}
				workspacePath="/work"
				schemeId="oqto-dark"
			/>,
		);
		await screen.findByText("main");
		const button = screen.getByRole("button", { name: /Commit/ });
		expect(button).toBeDisabled();
		expect(commit).not.toHaveBeenCalled();
	});

	it("commits the staged set with the typed message", async () => {
		const commit = vi.fn(async () => "abc123");
		render(
			<GitPane
				gitHost={host({ commit })}
				workspacePath="/work"
				schemeId="oqto-dark"
			/>,
		);
		await screen.findByText("main");
		fireEvent.change(screen.getByLabelText("Commit message"), {
			target: { value: "Fix the thing" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Commit 2 files" }));
		await waitFor(() =>
			expect(commit).toHaveBeenCalledWith("/work", "Fix the thing"),
		);
	});

	it("switches to a branch the tree is not on", async () => {
		const switchTo = vi.fn(async () => {});
		render(
			<GitPane
				gitHost={host({
					branches: {
						async list() {
							return [
								{ name: "main", current: true, worktree: "/work" },
								{ name: "spike", current: false, worktree: null },
							];
						},
						switch: switchTo,
					},
				})}
				workspacePath="/work"
				schemeId="oqto-dark"
			/>,
		);
		await screen.findByText("main");
		fireEvent.click(screen.getByRole("button", { name: "Switch branch" }));
		fireEvent.click(await screen.findByRole("button", { name: "spike" }));
		await waitFor(() =>
			expect(switchTo).toHaveBeenCalledWith("/work", "spike"),
		);
	});

	it("marks a branch another checkout already holds", async () => {
		render(
			<GitPane gitHost={host()} workspacePath="/work" schemeId="oqto-dark" />,
		);
		await screen.findByText("main");
		fireEvent.click(screen.getByRole("button", { name: "Switch branch" }));
		expect(await screen.findByText("in another checkout")).toBeInTheDocument();
	});

	it("reports what a remote operation said", async () => {
		const remote = vi.fn(async () => ({
			operation: "push" as const,
			summary: "everything up-to-date",
		}));
		render(
			<GitPane
				gitHost={host({ remote })}
				workspacePath="/work"
				schemeId="oqto-dark"
			/>,
		);
		await screen.findByText("main");
		fireEvent.click(screen.getByRole("button", { name: "Push" }));
		await waitFor(() => expect(remote).toHaveBeenCalledWith("/work", "push"));
		expect(
			await screen.findByText("everything up-to-date"),
		).toBeInTheDocument();
	});

	it("surfaces a refused switch rather than pretending it worked", async () => {
		render(
			<GitPane
				gitHost={host({
					branches: {
						async list() {
							return [
								{ name: "main", current: true, worktree: null },
								{ name: "spike", current: false, worktree: null },
							];
						},
						async switch() {
							throw new Error(
								"the working tree has changes; commit or stash first",
							);
						},
					},
				})}
				workspacePath="/work"
				schemeId="oqto-dark"
			/>,
		);
		await screen.findByText("main");
		fireEvent.click(screen.getByRole("button", { name: "Switch branch" }));
		fireEvent.click(await screen.findByRole("button", { name: "spike" }));
		expect(
			await screen.findByText(
				"the working tree has changes; commit or stash first",
			),
		).toBeInTheDocument();
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
				schemeId="oqto-dark"
			/>,
		);
		expect(await screen.findByText("not a git repository")).toBeInTheDocument();
	});
});
