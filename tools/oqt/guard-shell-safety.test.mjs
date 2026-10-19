import assert from "node:assert/strict";
import { access, mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import test from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import { withGuard } from "./guard-fixture.mjs";
import { WireClient } from "./guard-wire.ts";

test(
	"stale remember cannot change persistent rules",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({ tool, session, guard, requests, operator }) => {
				const operation = {
					request_id: `${session}:store:stale`,
					target: { capability: "execute", resource_id: "workspace-tools" },
					arguments: { tool: "bash", input: { command: "echo ok" } },
				};
				const abort = new AbortController();
				const pending = tool
					.call("prepare", { operation }, abort.signal)
					.catch(() => {});
				while (!requests.length) await sleep(10);
				const request = requests[0];
				const before = guard.rules.load();
				await assert.rejects(
					operator.call("decide", {
						session,
						...request,
						digest: "wrong",
						allow: true,
						lifetime: "once",
						remember: true,
					}),
				);
				assert.deepEqual(guard.rules.load(), before);
				abort.abort();
				await pending;
			},
			{ rules: true, decide: async () => {} },
		);
	},
);

test(
	"model lifetime excludes operator shell and drain",
	{ timeout: 20000 },
	async () =>
		withGuard(async ({ tool, session, operator, workspace, conversation }) => {
			const operation = {
				request_id: `${session}:store:hold`,
				target: { capability: "execute", resource_id: "workspace-tools" },
				arguments: { tool: "read", input: { path: "x" } },
			};
			await tool.call("prepare", { operation });
			const shell = operator.call("shell", {
				session,
				conversationId: conversation.id,
				command: "touch serialized-marker",
			});
			await sleep(150);
			await assert.rejects(access(join(workspace, "serialized-marker")));
			await tool.call("drain");
			await tool.call("check", { operation });
			await tool.call("finish", { id: operation.request_id });
			await shell;
			await access(join(workspace, "serialized-marker"));
		}),
);

test(
	"revoke interrupts operator descendants without waiting for shell timeout",
	{ timeout: 20000 },
	async () =>
		withGuard(
			async ({ operator, session, workspace, guardSession, conversation }) => {
				const running = operator
					.call("shell", {
						session,
						conversationId: conversation.id,
						command: "touch started; sleep 5; touch survived",
					})
					.catch((error) => error);
				while (true) {
					try {
						await access(join(workspace, "started"));
						break;
					} catch {
						await sleep(10);
					}
				}
				const before = Date.now();
				await operator.call("revoke_all", { session });
				assert.ok(Date.now() - before < 2000);
				assert.ok((await running) instanceof Error);
				await assert.rejects(access(join(workspace, "survived")));
				assert.equal((await guardSession.authority.recover()).length, 1);
			},
		),
);

test(
	"operator rechecks resource snapshot before an idle expired mount can run",
	{ timeout: 20000 },
	async () =>
		withGuard(
			async ({ root, tool, session, operator, guardSession, conversation }) => {
				const outside = join(root, "outside");
				await mkdir(outside);
				await writeFile(join(outside, "secret"), "expired-secret");
				await tool.call("request_resource", {
					request_id: `${session}:store:resource`,
					path: outside,
					access: "read",
					reason: "test",
				});
				const before = await operator.call("shell", {
					session,
					conversationId: conversation.id,
					command: `cat '${outside}/secret'`,
				});
				assert.match(before.output, /expired-secret/);
				// Fault-inject the protected authority's post-expiry snapshot, without
				// mutating its database or waiting for a real multi-minute resource lease.
				const resources = guardSession.authority.resources.bind(
					guardSession.authority,
				);
				guardSession.authority.resources = async () => ({
					...(await resources()),
					grants: [],
				});
				const after = await operator.call("shell", {
					session,
					conversationId: conversation.id,
					command: `cat '${outside}/secret'`,
				});
				assert.ok(!after.output.includes("expired-secret"));
			},
			{
				decide: async (request, operator) =>
					operator.call("decide", {
						session: request.session,
						...request,
						allow: true,
						lifetime: "session",
					}),
			},
		),
);

test(
	"unconfirmed model drain freezes authority and preserves its journal",
	{ timeout: 20000 },
	async () =>
		withGuard(async ({ tool, session, guardSession }) => {
			const operation = {
				request_id: `${session}:store:bad-drain`,
				target: { capability: "execute", resource_id: "workspace-tools" },
				arguments: { tool: "read", input: { path: "x" } },
			};
			await tool.call("prepare", { operation });
			const replace = guardSession.supervisor.replace.bind(
				guardSession.supervisor,
			);
			guardSession.supervisor.replace = async () => {
				throw new Error("injected stop uncertainty");
			};
			await assert.rejects(tool.call("drain"));
			await assert.rejects(guardSession.authority.ready());
			assert.equal((await guardSession.authority.recover()).length, 1);
			guardSession.supervisor.replace = replace;
		}),
);

test(
	"cancel running operator shell drains descendants",
	{ timeout: 20000 },
	async () =>
		withGuard(async ({ operator, session, workspace, conversation }) => {
			const abort = new AbortController();
			const running = operator.call(
				"shell",
				{
					session,
					conversationId: conversation.id,
					command: "touch cancel-started; sleep 3; touch cancel-survived",
				},
				abort.signal,
			);
			while (true) {
				try {
					await access(join(workspace, "cancel-started"));
					break;
				} catch {
					await sleep(10);
				}
			}
			abort.abort();
			await assert.rejects(running);
			await sleep(200);
			await assert.rejects(access(join(workspace, "cancel-survived")));
		}),
);

test(
	"live operator invocation recovers as interrupted, never implicitly replayed",
	{ timeout: 20000 },
	async () =>
		withGuard(
			async ({ operator, session, workspace, guardSession, conversation }) => {
				const running = operator
					.call("shell", {
						session,
						conversationId: conversation.id,
						command: "touch journal-started; sleep 5; touch journal-replayed",
					})
					.catch((error) => error);
				while (true) {
					try {
						await access(join(workspace, "journal-started"));
						break;
					} catch {
						await sleep(10);
					}
				}
				await guardSession.supervisor.stop();
				const interrupted = await guardSession.authority.recover();
				assert.equal(interrupted.length, 1);
				assert.match(interrupted[0].operation.request_id, /:operator:/);
				await running;
				await assert.rejects(access(join(workspace, "journal-replayed")));
			},
		),
);

test("canceled queued shell never starts", { timeout: 20000 }, async () =>
	withGuard(async ({ operator, tool, session, workspace, conversation }) => {
		const operation = {
			request_id: `${session}:store:queued-cancel`,
			target: { capability: "execute", resource_id: "workspace-tools" },
			arguments: { tool: "read", input: { path: "x" } },
		};
		await tool.call("prepare", { operation });
		const abort = new AbortController();
		const queued = operator
			.call(
				"shell",
				{
					session,
					conversationId: conversation.id,
					command: "touch canceled-marker",
				},
				abort.signal,
			)
			.catch((error) => error);
		await sleep(50);
		abort.abort();
		assert.ok((await queued) instanceof Error);
		await tool.call("drain");
		await tool.call("check", { operation });
		await tool.call("finish", { id: operation.request_id });
		await sleep(400);
		await assert.rejects(access(join(workspace, "canceled-marker")));
	}),
);

test(
	"shell escaped Unicode output fits encoded response frame",
	{ timeout: 20000 },
	async () =>
		withGuard(async ({ operator, session, conversation }) => {
			for (const codepoint of [65, 0, 34, 92, 28450, 128512]) {
				const result = await operator.call("shell", {
					session,
					conversationId: conversation.id,
					command: `python3 -c 'print(chr(${codepoint})*70000)'`,
				});
				assert.equal(result.exitCode, 0);
				assert.equal(result.truncated, true);
				assert.ok(
					Buffer.byteLength(
						`${JSON.stringify({ id: 100, ok: true, result })}\n`,
					) <= 65536,
				);
				assert.ok(await operator.call("subscribe"));
			}
		}),
);

test("disconnect queued shell never starts", { timeout: 20000 }, async () =>
	withGuard(async ({ operator, session, workspace, guard, conversation }) => {
		const first = operator.call("shell", {
			session,
			conversationId: conversation.id,
			command: "sleep 0.5",
		});
		await sleep(100);
		const other = await WireClient.connect(guard.operatorPath);
		const queued = other
			.call("shell", {
				session,
				conversationId: conversation.id,
				command: "touch disconnected-marker",
			})
			.catch(() => {});
		await sleep(50);
		other.close();
		await queued;
		await first;
		await sleep(400);
		await assert.rejects(access(join(workspace, "disconnected-marker")));
	}),
);
