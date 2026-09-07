/**
 * Live file system over the multiplexed WebSocket files channel. Requests
 * are correlated by id, watches are re-armed after a reconnect, and the
 * socket is opened lazily on first use.
 */

import type { FileChange, FileEntry, FileSystem } from "./files-contract";

interface FilesEvent {
	readonly channel?: string;
	readonly type?: string;
	readonly id?: string;
	readonly error?: string;
	readonly entries?: readonly {
		name?: string;
		is_dir?: boolean;
		is_symlink?: boolean;
		size?: number;
		modified_at?: number;
	}[];
	readonly path?: string;
	readonly event_type?: string;
	readonly entry_type?: string;
	readonly workspace_path?: string;
}

const CHANGE_KINDS: { readonly [event: string]: FileChange["kind"] } = {
	file_created: "created",
	dir_created: "created",
	file_modified: "modified",
	file_deleted: "deleted",
	dir_deleted: "deleted",
};

/** A files-channel command; the channel tag is added when framing. */
interface FilesCommand {
	readonly type: string;
	readonly id?: string;
	readonly path?: string;
	readonly include_hidden?: boolean;
	readonly workspace_path?: string;
}

/** The subset of WebSocket the adapter uses; a fake socket satisfies it. */
export interface MuxSocket {
	send(data: string): void;
	close(): void;
	onopen: ((this: unknown, event: unknown) => unknown) | null;
	onmessage: ((this: unknown, event: { data: unknown }) => unknown) | null;
	onclose: ((this: unknown, event: unknown) => unknown) | null;
}

export type SocketFactory = () => MuxSocket;

interface Pending {
	readonly resolve: (entries: readonly FileEntry[]) => void;
	readonly reject: (error: Error) => void;
}

function toEntry(
	directory: string,
	raw: NonNullable<FilesEvent["entries"]>[number],
): FileEntry {
	const name = raw.name ?? "";
	return {
		path: directory === "" ? name : `${directory}/${name}`,
		name,
		directory: raw.is_dir === true,
		symlink: raw.is_symlink === true,
		size: typeof raw.size === "number" ? raw.size : 0,
		modifiedAt: typeof raw.modified_at === "number" ? raw.modified_at : 0,
	};
}

export function createMuxFileSystem(openSocket: SocketFactory): FileSystem {
	let socket: MuxSocket | null = null;
	let open = false;
	const queue: string[] = [];
	const pending = new Map<string, Pending>();
	const watchers = new Map<string, Set<(change: FileChange) => void>>();
	let nextId = 0;

	const send = (payload: FilesCommand) => {
		const frame = JSON.stringify({ channel: "files", ...payload });
		if (open && socket) socket.send(frame);
		else {
			queue.push(frame);
			connect();
		}
	};

	function connect(): void {
		if (socket) return;
		const created = openSocket();
		socket = created;
		created.onopen = () => {
			open = true;
			for (const workspacePath of watchers.keys()) {
				created.send(
					JSON.stringify({
						channel: "files",
						type: "watch_files",
						workspace_path: workspacePath,
					}),
				);
			}
			for (const frame of queue.splice(0)) created.send(frame);
		};
		created.onmessage = (event) => {
			let message: FilesEvent;
			try {
				message = JSON.parse(String(event.data)) as FilesEvent;
			} catch {
				return;
			}
			if (message.channel !== "files") return;
			if (message.type === "file_changed") {
				const kind = CHANGE_KINDS[message.event_type ?? ""];
				const listeners = watchers.get(message.workspace_path ?? "");
				if (kind && listeners && message.path !== undefined) {
					for (const listener of listeners) {
						listener({
							path: message.path,
							kind,
							directory: message.entry_type === "directory",
						});
					}
				}
				return;
			}
			const waiting = message.id ? pending.get(message.id) : undefined;
			if (!waiting || !message.id) return;
			pending.delete(message.id);
			if (message.type === "error")
				waiting.reject(new Error(message.error ?? "files error"));
			else if (message.type === "list_result") {
				const directory = message.path ?? "";
				waiting.resolve(
					(message.entries ?? []).map((raw) => toEntry(directory, raw)),
				);
			}
		};
		created.onclose = () => {
			open = false;
			socket = null;
			for (const waiting of pending.values())
				waiting.reject(new Error("files socket closed"));
			pending.clear();
			// Re-arm watches on the next request; listeners stay registered.
			if (watchers.size > 0) connect();
		};
	}

	return {
		list(workspacePath, path) {
			nextId += 1;
			const id = `files-${nextId}`;
			return new Promise<readonly FileEntry[]>((resolve, reject) => {
				pending.set(id, { resolve, reject });
				send({
					type: "list",
					id,
					path,
					include_hidden: false,
					workspace_path: workspacePath,
				});
			});
		},
		watch(workspacePath, onChange) {
			const listeners = watchers.get(workspacePath) ?? new Set();
			listeners.add(onChange);
			watchers.set(workspacePath, listeners);
			send({ type: "watch_files", workspace_path: workspacePath });
			return () => {
				listeners.delete(onChange);
				if (listeners.size === 0) {
					watchers.delete(workspacePath);
					send({ type: "unwatch_files", workspace_path: workspacePath });
				}
			};
		},
	};
}
