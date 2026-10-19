import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtemp, rm } from "node:fs/promises";
import { connect } from "node:net";
import { join } from "node:path";
import test from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import { MAX_FRAME, serve, WireClient } from "./guard-wire.ts";

test(
	"wire cancellation never dispatches an already canceled queued handler",
	{ timeout: 5000 },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-queue-");
		const path = join(root, "wire.sock");
		let release;
		const hold = new Promise((resolve) => {
			release = resolve;
		});
		let calls = 0;
		const server = await serve(
			path,
			async (method) => {
				calls++;
				if (method === "hold") await hold;
				return true;
			},
			{ concurrency: 1 },
		);
		const client = await WireClient.connect(path);
		try {
			const first = client.call("hold");
			const abort = new AbortController();
			const queued = client.call("queued", {}, abort.signal);
			await sleep(30);
			abort.abort();
			await assert.rejects(queued);
			await sleep(30);
			release();
			await first;
			await sleep(30);
			assert.equal(calls, 1);
		} finally {
			release();
			client.close();
			await new Promise((resolve) => server.close(resolve));
			await rm(root, { recursive: true, force: true });
		}
	},
);

test(
	"oversized complete and split multibyte frames are refused before dispatch",
	{ timeout: 5000 },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-framing-");
		const path = join(root, "wire.sock");
		let calls = 0;
		const server = await serve(path, async () => {
			calls++;
			return "ok";
		});
		const sockets = [];
		try {
			for (const payload of [
				"x".repeat(MAX_FRAME),
				"漢".repeat(MAX_FRAME / 2),
			]) {
				const socket = connect(path);
				sockets.push(socket);
				socket.on("error", () => {});
				await once(socket, "connect");
				const closed = new Promise((resolve) => socket.once("close", resolve));
				socket.write(
					`${JSON.stringify({
						id: 1,
						method: "invalid-large",
						params: { payload },
					})}\n`,
				);
				await closed;
				assert.equal(calls, 0);
			}
			const valid = await WireClient.connect(path);
			try {
				assert.equal(await valid.call("ping"), "ok");
				assert.equal(calls, 1);
			} finally {
				valid.close();
			}
		} finally {
			for (const socket of sockets) socket.destroy();
			await new Promise((resolve) => server.close(resolve));
			await rm(root, { recursive: true, force: true });
		}
	},
);
