import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import {
	call,
	fauxAssistantMessage,
	toolResults,
	turn,
	withGuard,
} from "./guard-fixture.mjs";

test(
	"native coding waits for exact guard approvals and continues the same turn",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({ workspace, faux, conversation, requests, tool }) => {
				faux.setResponses([
					call("write", { path: "code.txt", content: "before\n" }, "write"),
					call(
						"edit",
						{ path: "code.txt", oldText: "before", newText: "after" },
						"edit",
					),
					call("read", { path: "code.txt" }, "read"),
					call("bash", { command: "cat code.txt" }, "bash"),
					fauxAssistantMessage("done"),
				]);
				const entries = await turn(conversation);
				// Reads inside the workspace are seamless; edits and shell ask.
				assert.deepEqual(
					requests.map((request) => [request.capability, request.category]),
					[
						["write", "edit"],
						["edit", "edit"],
						["bash", "shell"],
					],
				);
				assert.equal(requests[2].prefix, "cat");
				assert.equal(
					await readFile(join(workspace, "code.txt"), "utf8"),
					"after\n",
				);
				assert.ok(JSON.stringify(entries).includes("after"));
				assert.ok(toolResults(entries).every((message) => !message.isError));
				// The brain's tool channel cannot reach operator functions.
				for (const method of [
					"decide",
					"revoke_all",
					"acknowledge",
					"subscribe",
				])
					await assert.rejects(
						tool.call(method, { session: "x" }),
						/unavailable/,
					);
			},
		);
	},
);

test(
	"revocation during actual shell output withholds progress and final secret from durable history",
	{ timeout: 20000 },
	async () => {
		let revoked;
		await withGuard(
			async ({ workspace, faux, conversation }) => {
				await writeFile(
					join(workspace, "private.txt"),
					"revocation-secret-7249",
				);
				faux.setResponses([
					call(
						"bash",
						{ command: "cat private.txt; sleep 1; echo finished" },
						"revoked-shell",
					),
					fauxAssistantMessage("done"),
				]);
				const entries = await turn(conversation);
				await revoked;
				assert.ok(!JSON.stringify(entries).includes("revocation-secret-7249"));
				const result = toolResults(entries).find(
					(m) => m.toolCallId === "revoked-shell",
				);
				assert.ok(result?.isError);
				assert.ok(JSON.stringify(result).includes("uncertain"));
			},
			{
				decide: async (request, operator) => {
					await operator.call("decide", {
						session: request.session,
						...request,
						allow: true,
						lifetime: "session",
					});
					revoked = sleep(600).then(() =>
						operator.call("revoke_all", { session: request.session }),
					);
				},
			},
		);
	},
);

test(
	"hard coding ceiling denies before filesystem effects and ignores operator approval",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({ workspace, faux, conversation, requests }) => {
				faux.setResponses([
					call(
						"write",
						{ path: "forbidden.txt", content: "bad" },
						"denied-write",
					),
					fauxAssistantMessage("done"),
				]);
				const entries = await turn(conversation);
				assert.equal(requests.length, 0);
				await assert.rejects(readFile(join(workspace, "forbidden.txt")), {
					code: "ENOENT",
				});
				assert.ok(
					toolResults(entries).some(
						(m) => m.toolCallId === "denied-write" && m.isError,
					),
				);
			},
			{ policy: "deny" },
		);
	},
);

test(
	"a lost brain marks its live call interrupted, stops hands and fences Session grants",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({ workspace, guardSession, operator, tool, session }) => {
				const operation = (id, command) => ({
					request_id: `${session}:store:root:${id}`,
					target: { capability: "execute", resource_id: "workspace-tools" },
					arguments: { tool: "bash", input: { command } },
				});
				// Approve with a Session grant through the operator path.
				const first = operation("task-1", "sleep 30");
				const pending = tool.call("prepare", { operation: first });
				for (let i = 0; i < 100 && guardSession.pending.length === 0; i++)
					await sleep(20);
				const [request] = guardSession.state().pending;
				await operator.call("decide", {
					session,
					...request,
					allow: true,
					lifetime: "session",
				});
				const admitted = await pending;
				assert.match(admitted.epoch, /^[a-f0-9]{32}$/);
				// The brain disappears while the invocation is live.
				tool.close();
				for (let i = 0; i < 200 && guardSession.interrupted.length === 0; i++)
					await sleep(20);
				assert.equal(guardSession.interrupted.length, 1);
				assert.equal(
					guardSession.interrupted[0].operation.request_id,
					first.request_id,
				);
				assert.notEqual(
					guardSession.supervisor.epoch,
					admitted.epoch,
					"hands replaced",
				);
				// A new brain's retry is not covered by the old Session grant.
				const { WireClient } = await import("./guard-wire.ts");
				const next = await WireClient.connect(guardSession.toolPath);
				try {
					const retry = next.call("prepare", {
						operation: operation("task-2", "true"),
					});
					for (let i = 0; i < 100 && guardSession.pending.length === 0; i++)
						await sleep(20);
					assert.equal(
						guardSession.pending.length,
						1,
						"retry needs fresh approval",
					);
					const [again] = guardSession.state().pending;
					await operator.call("decide", {
						session,
						...again,
						allow: false,
						lifetime: "once",
					});
					await assert.rejects(retry, /denied/i);
					await operator.call("acknowledge", { session });
					assert.equal(guardSession.interrupted.length, 0);
				} finally {
					next.close();
				}
				void workspace;
			},
			{ decide: () => {} },
		);
	},
);
