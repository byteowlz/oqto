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

test(
	"guard returns success only after native receipt admission with Session-bound result lookup",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({ operator, session, operatorResults, conversation, faux }) => {
				const result = await operator.call("shell", {
					session,
					command: "printf durable-shared-value",
					excluded: false,
				});
				assert.equal(result.durable, true);
				assert.match(result.id, /:operator:/);
				const rows = await operatorResults.list({ session }, context);
				assert.equal(rows.length, 1);
				assert.equal(rows[0].id, result.id);
				assert.equal(rows[0].payload.output, result.output);
				assert.equal(faux.state.callCount, 0);
				assert.match(
					JSON.stringify((await conversation.context(context)).messages),
					/durable-shared-value/,
				);
				await assert.rejects(
					operatorResults.list({ session: "other-session" }, context),
				);
			},
		);
	},
);

test(
	"protected receipt quota is enforced before any new command effect",
	{ timeout: 20000 },
	async () => {
		await withGuard(async ({ guardSession, session, operator, workspace }) => {
			for (let index = 0; index < MAX_OPERATOR_RECEIPTS; index++)
				await guardSession.authority.operatorBegin(
					`${session}:operator:quota-${index}`,
					"true",
				);
			await assert.rejects(
				operator.call("shell", { session, command: "touch capacity-escaped" }),
				/capacity/,
			);
			await assert.rejects(access(join(workspace, "capacity-escaped")));
		});
	},
);

test(
	"active inference cannot deadlock operator admission; queued context places without a second run",
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
						operator.call("shell", {
							session,
							command: "printf queued-durable-context",
						}),
						new Promise((_, reject) => {
							deadline = setTimeout(
								() =>
									reject(
										new Error("passive receipt admission waited for placement"),
									),
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
				const rows = await operatorResults.list({ session }, context);
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
	"!! persists only opaque receipt identity, never private command/output in native history",
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
				const delivered = [];
				const off = tool.on("operator_result", (receipt) =>
					delivered.push(receipt),
				);
				const privateValue = "private-output-never-native-7461";
				const result = await operator.call("shell", {
					session,
					command: `printf ${privateValue}`,
					excluded: true,
				});
				assert.equal(result.output, privateValue);
				off();
				assert.equal(
					JSON.stringify(delivered).includes(privateValue),
					false,
					"private bytes must not cross into the brain",
				);
				const rows = await operatorResults.list({ session }, context);
				assert.equal(rows[0].id, result.id);
				assert.equal(rows[0].excluded, true);
				assert.equal(rows[0].payload, undefined);
				assert.equal(JSON.stringify(rows).includes(privateValue), false);
				assert.equal(
					JSON.stringify(
						(await conversation.context(context)).messages,
					).includes(privateValue),
					false,
				);
				assert.equal(faux.state.callCount, 0);
			},
		);
	},
);

test(
	"native admission followed by lost guard ACK recovers by sealed ID without replaying the effect",
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
				const original = tool.call.bind(tool);
				tool.call = async (method, ...args) => {
					if (method === "operator_result_committed")
						throw new Error("injected ACK transport loss");
					return original(method, ...args);
				};
				await assert.rejects(
					operator.call("shell", {
						session,
						command:
							"printf once >> receipt-effect; printf committed-before-ack",
					}),
					/not acknowledged/,
				);
				await operatorResults.drain();
				const rows = await operatorResults.list({ session }, context);
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
						{ harness, conversation, session, guard: replacement },
						context,
					);
					assert.equal((await guardSession.authority.recover()).length, 0);
					assert.equal(
						(await receipts.list({ session }, context))[0].id,
						rows[0].id,
					);
					assert.equal(
						await readFile(join(workspace, "receipt-effect"), "utf8"),
						"once",
					);
					assert.equal(faux.state.callCount, 0);
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
						operator.call("shell", {
							session,
							command: "touch unacknowledged-effect; printf undelivered",
						}),
						/not acknowledged/,
					);
				} finally {
					harness.commit = commit;
				}
				await access(join(workspace, "unacknowledged-effect"));
				assert.equal((await guardSession.authority.recover()).length, 1);
				assert.equal(
					(await operatorResults.list({ session }, context)).length,
					0,
				);
			},
		);
	},
);

test(
	"unconfirmed operator teardown freezes authority and cannot publish a result",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({ operator, session, operatorResults, guardSession }) => {
				const replace = guardSession.supervisor.replace.bind(
					guardSession.supervisor,
				);
				guardSession.supervisor.replace = async () => {
					throw new Error("injected unconfirmed stop");
				};
				try {
					await assert.rejects(
						operator.call("shell", {
							session,
							command: "printf withheld-stop-result",
						}),
					);
					await assert.rejects(guardSession.authority.ready());
					assert.equal((await guardSession.authority.recover()).length, 1);
					assert.equal(
						(await operatorResults.list({ session }, context)).length,
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
	"resource expiry at the post-drain publication boundary withholds native disclosure",
	{ timeout: 20000 },
	async () => {
		await withGuard(
			async ({
				root,
				tool,
				session,
				operator,
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
					operator.call("shell", {
						session,
						command: `cat '${outside}/secret'`,
					}),
					/not acknowledged/,
				);
				assert.equal(
					(await operatorResults.list({ session }, context)).length,
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
