/**
 * Live version control over the multiplexed WebSocket git channel. Requests
 * are correlated by id and the socket opens lazily, the same shape as the
 * files and trx adapters; the channel's wire record is mapped to the port's
 * here so no pane ever sees snake_case.
 */

import type {
	GitBranch,
	GitCommit,
	GitDiff,
	GitEntry,
	GitHost,
	GitStatus,
	RemoteOutcome,
} from "./git-contract";
import type { MuxSocket } from "./mux-files";

interface GitEvent {
	readonly channel?: string;
	readonly type?: string;
	readonly id?: string;
	readonly error?: string;
	readonly branch?: string;
	readonly upstream?: string;
	readonly ahead?: number;
	readonly behind?: number;
	readonly truncated?: boolean;
	readonly entries?: readonly {
		path?: string;
		index?: string;
		worktree?: string;
		renamed_from?: string;
	}[];
	readonly path?: string;
	readonly staged?: boolean;
	readonly patch?: string;
	readonly commit?: string;
	readonly current?: string;
	readonly operation?: string;
	readonly summary?: string;
	readonly branches?: readonly {
		name?: string;
		current?: boolean;
		worktree?: string;
	}[];
	readonly commits?: readonly {
		id?: string;
		short_id?: string;
		summary?: string;
		author?: string;
		timestamp?: number;
	}[];
}

/** A git-channel command; the channel tag and id are added when framing. */
interface GitCommand {
	readonly type: string;
	readonly workspace_path: string;
	readonly path?: string;
	readonly staged?: boolean;
	readonly limit?: number;
	readonly paths?: readonly string[];
	readonly message?: string;
	readonly branch?: string;
}

type PendingValue =
	| GitStatus
	| GitDiff
	| readonly GitCommit[]
	| readonly GitBranch[]
	| RemoteOutcome
	| string
	| undefined;

interface Pending {
	readonly resolve: (value: PendingValue) => void;
	readonly reject: (error: Error) => void;
	readonly expect: string;
	readonly timer: ReturnType<typeof setTimeout>;
}

/**
 * A request the host never answers must not spin forever. A host without
 * this channel answers on `system` instead, which is why those errors fail
 * the requests in flight rather than being ignored.
 */
const REQUEST_TIMEOUT_MS = 30_000;

function toStatus(message: GitEvent): GitStatus {
	return {
		branch: message.branch ?? "",
		upstream: message.upstream ?? null,
		ahead: message.ahead ?? 0,
		behind: message.behind ?? 0,
		truncated: message.truncated === true,
		entries: (message.entries ?? []).map(
			(raw): GitEntry => ({
				path: raw.path ?? "",
				index: raw.index ?? " ",
				worktree: raw.worktree ?? " ",
				renamedFrom: raw.renamed_from ?? null,
			}),
		),
	};
}

export function createMuxGitHost(openSocket: () => MuxSocket): GitHost {
	let socket: MuxSocket | null = null;
	let open = false;
	const queue: string[] = [];
	const pending = new Map<string, Pending>();
	let nextId = 0;

	function connect(): void {
		if (socket) return;
		const created = openSocket();
		socket = created;
		created.onopen = () => {
			open = true;
			for (const frame of queue.splice(0)) created.send(frame);
		};
		created.onmessage = (event) => {
			let message: GitEvent;
			try {
				message = JSON.parse(String(event.data)) as GitEvent;
			} catch {
				return;
			}
			if (message.channel === "system" && message.type === "error") {
				failAll(new Error(message.error ?? "connection error"));
				return;
			}
			if (message.channel !== "git" || !message.id) return;
			const waiting = pending.get(message.id);
			if (!waiting) return;
			if (message.type === "error") {
				settle(message.id);
				waiting.reject(new Error(message.error ?? "git error"));
				return;
			}
			if (message.type !== waiting.expect) return;
			settle(message.id);
			if (message.type === "status_result") {
				waiting.resolve(toStatus(message));
			} else if (message.type === "diff_result") {
				waiting.resolve({
					path: message.path ?? "",
					staged: message.staged === true,
					patch: message.patch ?? "",
					truncated: message.truncated === true,
				});
			} else if (message.type === "log_result") {
				waiting.resolve(
					(message.commits ?? []).map(
						(raw): GitCommit => ({
							id: raw.id ?? "",
							shortId: raw.short_id ?? "",
							summary: raw.summary ?? "",
							author: raw.author ?? "",
							// The channel reports Unix seconds; panes format milliseconds.
							timestamp: (raw.timestamp ?? 0) * 1000,
						}),
					),
				);
			} else if (message.type === "commit_result") {
				waiting.resolve(message.commit ?? "");
			} else if (message.type === "branches_result") {
				waiting.resolve(
					(message.branches ?? []).map(
						(raw): GitBranch => ({
							name: raw.name ?? "",
							current: raw.current === true,
							worktree: raw.worktree ?? null,
						}),
					),
				);
			} else if (message.type === "remote_result") {
				waiting.resolve({
					operation: (message.operation ??
						"fetch") as RemoteOutcome["operation"],
					summary: message.summary ?? "",
				});
			} else {
				waiting.resolve(undefined);
			}
		};
		created.onclose = () => {
			open = false;
			socket = null;
			failAll(new Error("git socket closed"));
		};
	}

	/** Clears a request's timer and forgets it. */
	function settle(id: string): void {
		const waiting = pending.get(id);
		if (!waiting) return;
		clearTimeout(waiting.timer);
		pending.delete(id);
	}

	function failAll(error: Error): void {
		for (const [id, waiting] of pending) {
			clearTimeout(waiting.timer);
			pending.delete(id);
			waiting.reject(error);
		}
	}

	function request<Value extends PendingValue>(
		expect: string,
		command: GitCommand,
	): Promise<Value> {
		nextId += 1;
		const id = `git-${nextId}`;
		return new Promise<Value>((resolve, reject) => {
			const timer = setTimeout(() => {
				pending.delete(id);
				reject(new Error(`git ${command.type} timed out`));
			}, REQUEST_TIMEOUT_MS);
			pending.set(id, {
				resolve: resolve as (value: PendingValue) => void,
				reject,
				expect,
				timer,
			});
			const frame = JSON.stringify({ channel: "git", id, ...command });
			if (open && socket) socket.send(frame);
			else {
				queue.push(frame);
				connect();
			}
		});
	}

	return {
		status(workspacePath) {
			return request<GitStatus>("status_result", {
				type: "status",
				workspace_path: workspacePath,
			});
		},
		diff(workspacePath, path, staged) {
			return request<GitDiff>("diff_result", {
				type: "diff",
				workspace_path: workspacePath,
				path,
				staged,
			});
		},
		log(workspacePath, limit) {
			return request<readonly GitCommit[]>("log_result", {
				type: "log",
				workspace_path: workspacePath,
				limit,
			});
		},
		stage(workspacePath, paths, staged) {
			return request<undefined>(staged ? "stage_result" : "unstage_result", {
				type: staged ? "stage" : "unstage",
				workspace_path: workspacePath,
				paths,
			});
		},
		branches: {
			list(workspacePath) {
				return request<readonly GitBranch[]>("branches_result", {
					type: "branches",
					workspace_path: workspacePath,
				});
			},
			switch(workspacePath, branch) {
				return request<undefined>("switch_result", {
					type: "switch",
					workspace_path: workspacePath,
					branch,
				});
			},
		},
		remote(workspacePath, operation) {
			return request<RemoteOutcome>("remote_result", {
				type: operation,
				workspace_path: workspacePath,
			});
		},
		commit(workspacePath, message) {
			return request<string>("commit_result", {
				type: "commit",
				workspace_path: workspacePath,
				message,
			});
		},
	};
}
