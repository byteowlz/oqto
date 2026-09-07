import {
	type MuxSocket,
	createMuxFileSystem,
} from "@/src/oqto-ui/platform/mux-files";
import { describe, expect, it } from "vitest";

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

describe("mux files adapter", () => {
	it("frames a list request on the files channel and resolves it by id", async () => {
		const { socket, sent } = fakeSocket();
		const files = createMuxFileSystem(() => socket);
		const pending = files.list("/work/repo", "src");
		socket.open();
		expect(sent[0]).toEqual({
			channel: "files",
			type: "list",
			id: "files-1",
			path: "src",
			include_hidden: false,
			workspace_path: "/work/repo",
		});
		socket.deliver({
			channel: "files",
			type: "list_result",
			id: "files-1",
			path: "src",
			entries: [
				{
					name: "app.ts",
					is_dir: false,
					is_symlink: false,
					size: 12,
					modified_at: 5,
				},
				{
					name: "lib",
					is_dir: true,
					is_symlink: false,
					size: 0,
					modified_at: 7,
				},
			],
		});
		await expect(pending).resolves.toEqual([
			{
				path: "src/app.ts",
				name: "app.ts",
				directory: false,
				symlink: false,
				size: 12,
				modifiedAt: 5,
			},
			{
				path: "src/lib",
				name: "lib",
				directory: true,
				symlink: false,
				size: 0,
				modifiedAt: 7,
			},
		]);
	});

	it("rejects the matching request on a channel error and ignores other channels", async () => {
		const { socket } = fakeSocket();
		const files = createMuxFileSystem(() => socket);
		const pending = files.list("/work/repo", "nope");
		socket.open();
		socket.deliver({
			channel: "agent",
			type: "error",
			id: "files-1",
			error: "unrelated",
		});
		socket.deliver({
			channel: "files",
			type: "error",
			id: "files-1",
			error: "no such directory",
		});
		await expect(pending).rejects.toThrow("no such directory");
	});

	it("watches a work directory, maps change kinds, and unwatches when the last listener leaves", () => {
		const { socket, sent } = fakeSocket();
		const files = createMuxFileSystem(() => socket);
		const seen: { path: string; kind: string; directory: boolean }[] = [];
		const stop = files.watch("/work/repo", (change) => seen.push(change));
		socket.open();
		expect(sent[0]).toEqual({
			channel: "files",
			type: "watch_files",
			workspace_path: "/work/repo",
		});
		socket.deliver({
			channel: "files",
			type: "file_changed",
			event_type: "file_modified",
			path: "src/app.ts",
			entry_type: "file",
			workspace_path: "/work/repo",
		});
		socket.deliver({
			channel: "files",
			type: "file_changed",
			event_type: "dir_created",
			path: "src/new",
			entry_type: "directory",
			workspace_path: "/other",
		});
		expect(seen).toEqual([
			{ path: "src/app.ts", kind: "modified", directory: false },
		]);
		stop();
		expect(sent.at(-1)).toEqual({
			channel: "files",
			type: "unwatch_files",
			workspace_path: "/work/repo",
		});
	});

	it("queues requests made before the socket opens and fails them if it closes", async () => {
		const { socket, sent } = fakeSocket();
		const files = createMuxFileSystem(() => socket);
		const pending = files.list("/work/repo", "");
		expect(sent).toHaveLength(0);
		socket.open();
		expect(sent).toHaveLength(1);
		socket.close();
		await expect(pending).rejects.toThrow("files socket closed");
	});
});
