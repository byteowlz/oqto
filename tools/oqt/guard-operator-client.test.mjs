import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import {
	BACKGROUND_CONTEXT,
	withAbortSignal,
} from "@earendil-works/chord/context";
import { serve } from "./guard-wire.ts";
import { guardPermissions } from "./plugin/src/guard-client.ts";

test(
	"operator adapter replays cached startup interruptions and forwards exact call cancellation",
	{ timeout: 5000 },
	async () => {
		const directory = await mkdtemp("/var/tmp/oqt-operator-port-");
		const path = join(directory, "operator.sock");
		let began;
		let cancelled;
		const started = new Promise((resolve) => {
			began = resolve;
		});
		const stopped = new Promise((resolve) => {
			cancelled = resolve;
		});
		const peers = new Set();
		const server = await serve(
			path,
			async (method, params, signal, peer) => {
				if (method === "subscribe") {
					peer.push("state", {
						sessions: [
							{
								id: "A",
								pending: [],
								resources: [],
								interrupted: [
									{
										id: "A:operator:stable-id",
										tool: "bash",
										argumentsText: "fixture",
										startedAt: 1,
									},
								],
							},
						],
					});
					return { subscribed: true };
				}
				assert.equal(method, "shell");
				assert.equal(params.session, "A");
				assert.equal(params.excluded, true);
				began();
				await new Promise((resolve) =>
					signal.addEventListener(
						"abort",
						() => {
							cancelled({ disconnected: peer.closed.aborted });
							resolve();
						},
						{ once: true },
					),
				);
				return { cancelled: true };
			},
			{ onConnect: (peer) => peers.add(peer) },
		);
		const client = guardPermissions(path);
		try {
			while (!client.state.value) await sleep(5);
			let observed;
			const off = client.state.subscribe((value) => {
				observed = value;
			});
			assert.equal(
				observed.interrupted[0].id,
				"A:operator:stable-id",
				"late subscriber must not miss startup recovery",
			);
			off();
			const abort = new AbortController();
			const active = client.shell(
				"A",
				"fixture",
				withAbortSignal(abort.signal, BACKGROUND_CONTEXT),
				true,
			);
			await started;
			abort.abort();
			await assert.rejects(active, /cancelled/);
			assert.equal(
				(await stopped).disconnected,
				false,
				"cancel the invocation, not the shared observation connection",
			);
		} finally {
			client.close();
			for (const peer of peers) peer.socket.destroy();
			await new Promise((resolve) => server.close(resolve));
			await rm(directory, { recursive: true, force: true });
		}
	},
);
