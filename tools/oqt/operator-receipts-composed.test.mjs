import assert from "node:assert/strict";
import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { context, fauxAssistantMessage, withGuard } from "./guard-fixture.mjs";
import { WireClient } from "./guard-wire.ts";
import {
	installOperatorReceipts,
	MAX_OPERATOR_RECEIPTS,
} from "./operator-receipts.ts";

const shell = (operator, session, conversationId, command, excluded = false) =>
	operator.call("shell", { session, conversationId, command, excluded });
const list = (receipts, session, conversationId) =>
	receipts.list({ session, conversationId }, context);

test(
	"guard admits passive v2 output to the explicit selected branch, rejecting stale clients",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({
				operator,
				session,
				operatorResults,
				conversation,
				harness,
				tool,
				faux,
				workspace,
			}) => {
				const anchor = await conversation.commit(
					(tx) =>
						tx.appendEntry(conversation.id, { kind: "fixture", data: {} }),
					context,
				);
				const child = await conversation.fork(
					anchor.id,
					{ ownership: { kind: "ownerless" } },
					context,
				);
				const { token } = await tool.call("operator_results_select_begin", {
					conversationId: conversation.id,
				});
				await assert.rejects(
					shell(operator, session, conversation.id, "touch selection-escaped"),
					/changing|busy/,
				);
				await tool.call("operator_results_select_end", {
					token,
					conversationId: child.id,
				});
				await assert.rejects(
					shell(operator, session, conversation.id, "touch selection-escaped"),
					/stale/,
				);
				await assert.rejects(access(join(workspace, "selection-escaped")));
				const result = await shell(
					operator,
					session,
					child.id,
					"printf durable-shared-value",
				);
				assert.equal(result.durable, true);
				const rows = await list(operatorResults, session, child.id);
				assert.equal(rows[0].id, result.id);
				assert.equal(rows[0].payload.output, result.output);
				assert.equal(rows[0].conversationId, child.id);
				assert.deepEqual(
					await list(operatorResults, session, conversation.id),
					[],
				);
				assert.match(
					JSON.stringify((await child.context(context)).messages),
					/durable-shared-value/,
				);
				assert.doesNotMatch(
					JSON.stringify((await conversation.context(context)).messages),
					/durable-shared-value/,
				);
				assert.equal(faux.state.callCount, 0);
				await assert.rejects(list(operatorResults, "other-session", child.id));
				// A live/reserved invocation fences navigation, not merely publication time.
				const pending = shell(
					operator,
					session,
					child.id,
					"sleep 0.2; printf live-fence",
				);
				await new Promise((resolve) => setTimeout(resolve, 30));
				await assert.rejects(
					tool.call("operator_results_select_begin", {
						conversationId: child.id,
					}),
					/live/,
				);
				await pending;
				const back = await tool.call("operator_results_select_begin", {
					conversationId: child.id,
				});
				await tool.call("operator_results_select_end", {
					token: back.token,
					conversationId: conversation.id,
				});
				assert.equal(
					(await list(operatorResults, session, child.id)).length,
					2,
				);
				assert.equal(
					(await list(operatorResults, session, conversation.id)).length,
					0,
				);
				assert.ok(harness);
			},
		);
	},
);

test(
	"protected receipt quota is Session-wide and checked before new effects even after navigation",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({
				guardSession,
				session,
				operator,
				workspace,
				conversation,
				tool,
			}) => {
				for (let i = 0; i < MAX_OPERATOR_RECEIPTS; i++)
					await guardSession.authority.operatorBegin(
						`${session}:operator:quota-${i}`,
						"true",
					);
				const anchor = await conversation.commit(
					(tx) =>
						tx.appendEntry(conversation.id, { kind: "fixture", data: {} }),
					context,
				);
				const child = await conversation.fork(
					anchor.id,
					{ ownership: { kind: "ownerless" } },
					context,
				);
				const { token } = await tool.call("operator_results_select_begin", {
					conversationId: conversation.id,
				});
				await tool.call("operator_results_select_end", {
					token,
					conversationId: child.id,
				});
				await assert.rejects(
					shell(operator, session, child.id, "touch capacity-escaped"),
					/capacity/,
				);
				await assert.rejects(access(join(workspace, "capacity-escaped")));
			},
		);
	},
);

test(
	"active inference cannot deadlock passive operator admission or start another model run",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({
				operator,
				session,
				operatorResults,
				conversation,
				harness,
				faux,
			}) => {
				let release;
				let started;
				const began = new Promise((resolve) => {
					started = resolve;
				});
				const held = new Promise((resolve) => {
					release = resolve;
				});
				faux.setResponses([
					async () => {
						started();
						await held;
						return fauxAssistantMessage("initial answer");
					},
				]);
				const initial = await conversation.submit(
					{
						type: "input",
						content: "hold inference",
						requestId: "held-inference",
					},
					context,
				);
				await began;
				let deadline;
				try {
					const result = await Promise.race([
						shell(
							operator,
							session,
							conversation.id,
							"printf queued-durable-context",
						),
						new Promise((_, reject) => {
							deadline = setTimeout(
								() =>
									reject(new Error("passive admission waited for placement")),
								5000,
							);
						}),
					]);
					assert.equal(result.durable, true);
					assert.equal(faux.state.callCount, 1);
				} finally {
					clearTimeout(deadline);
					release();
				}
				await initial.wait(context);
				const rows = await list(operatorResults, session, conversation.id);
				const submission = await harness.submission(
					rows[0].submission,
					context,
				);
				assert.equal((await submission.wait(context)).status, "done");
				assert.match(
					JSON.stringify((await conversation.context(context)).messages),
					/queued-durable-context/,
				);
				assert.equal(faux.state.callCount, 1);
			},
		);
	},
);

test(
	"!! crosses the worker boundary as opaque v2 metadata only, also on a selected fork",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({
				operator,
				session,
				operatorResults,
				conversation,
				faux,
				tool,
			}) => {
				const anchor = await conversation.commit(
					(tx) =>
						tx.appendEntry(conversation.id, { kind: "fixture", data: {} }),
					context,
				);
				const child = await conversation.fork(
					anchor.id,
					{ ownership: { kind: "ownerless" } },
					context,
				);
				const { token } = await tool.call("operator_results_select_begin", {
					conversationId: conversation.id,
				});
				await tool.call("operator_results_select_end", {
					token,
					conversationId: child.id,
				});
				const delivered = [];
				const off = tool.on("operator_result", (receipt) =>
					delivered.push(receipt),
				);
				const value = "private-output-never-native-7461";
				const result = await shell(
					operator,
					session,
					child.id,
					`printf ${value}`,
					true,
				);
				off();
				assert.equal(result.output, value);
				assert.equal(JSON.stringify(delivered).includes(value), false);
				assert.equal(delivered[0].version, 2);
				assert.equal(delivered[0].conversationId, child.id);
				const rows = await list(operatorResults, session, child.id);
				assert.equal(rows[0].id, result.id);
				assert.equal(rows[0].excluded, true);
				assert.equal(rows[0].payload, undefined);
				assert.equal(JSON.stringify(rows).includes(value), false);
				assert.equal(
					JSON.stringify((await child.context(context)).messages).includes(
						value,
					),
					false,
				);
				assert.deepEqual(
					await list(operatorResults, session, conversation.id),
					[],
				);
				assert.equal(faux.state.callCount, 0);
			},
		);
	},
);

test(
	"lost fork ACK recovers all sealed branch receipts while root is selected, never replays effects",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({
				operator,
				session,
				operatorResults,
				tool,
				guardSession,
				workspace,
				harness,
				conversation,
				faux,
			}) => {
				const anchor = await conversation.commit(
					(tx) =>
						tx.appendEntry(conversation.id, { kind: "fixture", data: {} }),
					context,
				);
				const child = await conversation.fork(
					anchor.id,
					{ ownership: { kind: "ownerless" } },
					context,
				);
				const selection = await tool.call("operator_results_select_begin", {
					conversationId: conversation.id,
				});
				await tool.call("operator_results_select_end", {
					token: selection.token,
					conversationId: child.id,
				});
				const original = tool.call.bind(tool);
				tool.call = async (method, ...args) => {
					if (method === "operator_result_committed")
						throw new Error("injected ACK loss");
					return original(method, ...args);
				};
				await assert.rejects(
					shell(
						operator,
						session,
						child.id,
						"printf once >> receipt-effect; printf committed-before-ack",
					),
					/not acknowledged/,
				);
				await operatorResults.drain();
				const rows = await list(operatorResults, session, child.id);
				assert.equal(rows.length, 1);
				assert.equal((await guardSession.authority.recover()).length, 1);
				await operatorResults.close();
				tool.call = original;
				tool.close();
				await tool.closed;
				const replacement = await WireClient.connect(guardSession.toolPath);
				let receipts;
				try {
					receipts = await installOperatorReceipts(
						{
							harness,
							selectedConversationId: () => conversation.id,
							session,
							guard: replacement,
						},
						context,
					);
					assert.equal((await guardSession.authority.recover()).length, 0);
					assert.equal(
						(await list(receipts, session, child.id))[0].id,
						rows[0].id,
					);
					assert.deepEqual(await list(receipts, session, conversation.id), []);
					assert.equal(
						await readFile(join(workspace, "receipt-effect"), "utf8"),
						"once",
					);
					assert.equal(faux.state.callCount, 0);
					assert.doesNotMatch(
						JSON.stringify((await conversation.context(context)).messages),
						/committed-before-ack/,
					);
				} finally {
					await receipts?.close();
					replacement.close();
				}
			},
		);
	},
);

test(
	"native commit failure withholds success and preserves an uncertain journal",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({
				operator,
				session,
				harness,
				conversation,
				operatorResults,
				guardSession,
				workspace,
			}) => {
				const commit = harness.commit.bind(harness);
				harness.commit = async () => {
					throw new Error("injected native storage failure");
				};
				try {
					await assert.rejects(
						shell(
							operator,
							session,
							conversation.id,
							"touch unacknowledged-effect; printf undelivered",
						),
						/not acknowledged/,
					);
				} finally {
					harness.commit = commit;
				}
				await access(join(workspace, "unacknowledged-effect"));
				assert.equal((await guardSession.authority.recover()).length, 1);
				assert.equal(
					(await list(operatorResults, session, conversation.id)).length,
					0,
				);
			},
		);
	},
);

test(
	"unconfirmed operator teardown freezes authority and cannot publish",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({
				operator,
				session,
				conversation,
				operatorResults,
				guardSession,
			}) => {
				const replace = guardSession.supervisor.replace.bind(
					guardSession.supervisor,
				);
				guardSession.supervisor.replace = async () => {
					throw new Error("injected unconfirmed stop");
				};
				try {
					await assert.rejects(
						shell(
							operator,
							session,
							conversation.id,
							"printf withheld-stop-result",
						),
					);
					await assert.rejects(guardSession.authority.ready());
					assert.equal((await guardSession.authority.recover()).length, 1);
					assert.equal(
						(await list(operatorResults, session, conversation.id)).length,
						0,
					);
				} finally {
					guardSession.supervisor.replace = replace;
				}
			},
		);
	},
);

test(
	"resource expiry at post-drain publication withholds native disclosure",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({
				root,
				tool,
				session,
				operator,
				conversation,
				guardSession,
				operatorResults,
			}) => {
				const outside = join(root, "publication-resource");
				await mkdir(outside);
				await writeFile(join(outside, "secret"), "expired-before-publication");
				await tool.call("request_resource", {
					request_id: `${session}:store:publication-resource`,
					path: outside,
					access: "read",
					reason: "fixture",
				});
				const resources = guardSession.authority.resources.bind(
					guardSession.authority,
				);
				const replace = guardSession.supervisor.replace.bind(
					guardSession.supervisor,
				);
				let expired = false;
				guardSession.authority.resources = async () => ({
					...(await resources()),
					...(expired ? { grants: [] } : {}),
				});
				guardSession.supervisor.replace = async () => {
					await replace();
					expired = true;
				};
				await assert.rejects(
					shell(operator, session, conversation.id, `cat '${outside}/secret'`),
					/not acknowledged/,
				);
				assert.equal(
					(await list(operatorResults, session, conversation.id)).length,
					0,
				);
				assert.equal((await guardSession.authority.recover()).length, 1);
			},
			{
				decide: (request, operator) =>
					operator.call("decide", {
						session: request.session,
						...request,
						allow: true,
						lifetime: "session",
					}),
			},
		);
	},
);
