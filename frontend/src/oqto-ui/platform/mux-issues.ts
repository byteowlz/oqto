/**
 * Live issue tracker over the multiplexed WebSocket trx channel. Requests
 * are correlated by id and the socket opens lazily, the same shape as the
 * files adapter; the tracker's wire record is mapped to the port's here so
 * no pane ever sees snake_case.
 */

import type { Issue, IssueHost, IssueStatus } from "./issues-contract";
import type { MuxSocket } from "./mux-files";

interface IssueEvent {
	readonly channel?: string;
	readonly type?: string;
	readonly id?: string;
	readonly error?: string;
	readonly issues?: readonly {
		id?: string;
		title?: string;
		status?: string;
		priority?: number;
		issue_type?: string;
		parent_id?: string;
		blocked_by?: string[];
		labels?: string[];
	}[];
}

const STATUSES: ReadonlySet<string> = new Set([
	"open",
	"ready",
	"in_progress",
	"blocked",
	"closed",
]);

function toIssue(raw: NonNullable<IssueEvent["issues"]>[number]): Issue {
	const status = raw.status ?? "open";
	return {
		id: raw.id ?? "",
		title: raw.title ?? "",
		status: (STATUSES.has(status) ? status : "open") as IssueStatus,
		priority: typeof raw.priority === "number" ? raw.priority : 3,
		kind: raw.issue_type ?? "task",
		parentId: raw.parent_id ?? null,
		blockedBy: raw.blocked_by ?? [],
		labels: raw.labels ?? [],
	};
}

/** A trx-channel command; the channel tag and id are added when framing. */
interface IssueCommand {
	readonly type: string;
	readonly workspace_path: string;
	readonly issue_id?: string;
	readonly reason?: string;
	readonly data?: { readonly status: IssueStatus };
}

interface Pending {
	readonly resolve: (value: readonly Issue[] | undefined) => void;
	readonly reject: (error: Error) => void;
	readonly expect: string;
}

export function createMuxIssueHost(openSocket: () => MuxSocket): IssueHost {
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
			let message: IssueEvent;
			try {
				message = JSON.parse(String(event.data)) as IssueEvent;
			} catch {
				return;
			}
			if (message.channel !== "trx" || !message.id) return;
			const waiting = pending.get(message.id);
			if (!waiting) return;
			if (message.type === "error") {
				pending.delete(message.id);
				waiting.reject(new Error(message.error ?? "trx error"));
				return;
			}
			if (message.type !== waiting.expect) return;
			pending.delete(message.id);
			waiting.resolve(
				message.type === "list_result"
					? (message.issues ?? []).map(toIssue)
					: undefined,
			);
		};
		created.onclose = () => {
			open = false;
			socket = null;
			for (const waiting of pending.values())
				waiting.reject(new Error("trx socket closed"));
			pending.clear();
		};
	}

	function request<Value extends readonly Issue[] | undefined>(
		expect: string,
		command: IssueCommand,
	): Promise<Value> {
		nextId += 1;
		const id = `trx-${nextId}`;
		return new Promise<Value>((resolve, reject) => {
			pending.set(id, {
				resolve: resolve as (value: readonly Issue[] | undefined) => void,
				reject,
				expect,
			});
			const frame = JSON.stringify({ channel: "trx", id, ...command });
			if (open && socket) socket.send(frame);
			else {
				queue.push(frame);
				connect();
			}
		});
	}

	return {
		list(workspacePath) {
			return request<readonly Issue[]>("list_result", {
				type: "list",
				workspace_path: workspacePath,
			});
		},
		setStatus(workspacePath, issueId, status, reason) {
			if (status === "closed")
				return request<undefined>("issue_result", {
					type: "close",
					workspace_path: workspacePath,
					issue_id: issueId,
					reason,
				});
			return request<undefined>("issue_result", {
				type: "update",
				workspace_path: workspacePath,
				issue_id: issueId,
				data: { status },
			});
		},
	};
}
