import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import {
	call,
	context,
	fauxAssistantMessage,
	toolResults,
	withGuard,
} from "./guard-fixture.mjs";

test(
	"guard-composed coding revokes active hands, fences output, retires completed descendants and continues the same turn",
	{ timeout: 30000 },
	async () => {
		const controls = [];
		let shared;
		await withGuard(
			async ({ workspace, faux, conversation, requests }) => {
				shared = workspace;
				await writeFile(join(workspace, "private.txt"), "composed-secret-1962");
				faux.setResponses([
					call(
						"bash",
						{
							command:
								"cat private.txt; (while true; do echo tick >> heartbeat; sleep 0.05; done) & wait",
							timeout: 10,
						},
						"active-shell",
					),
					call(
						"write",
						{ path: "after-revoke", content: "fresh epoch" },
						"fresh-write",
					),
					call(
						"bash",
						{
							command:
								"setsid /bin/sh -c 'while true; do echo tick >> detached; sleep 0.05; done' >/dev/null 2>&1 </dev/null & while test ! -s detached; do sleep 0.01; done; exit 0",
						},
						"detached-completion",
					),
					fauxAssistantMessage("done"),
				]);
				const turn = await conversation.submit(
					{ type: "input", content: "run coding", requestId: "runtime-turn" },
					context,
				);
				assert.equal((await turn.wait(context)).status, "done");
				await Promise.all(controls);
				// Shell (session) -> revoked -> edit asks -> shell asks again.
				assert.deepEqual(
					requests.map((request) => request.category),
					["shell", "edit", "shell"],
				);
				const detached = await readFile(join(workspace, "detached"), "utf8");
				await sleep(150);
				assert.equal(
					await readFile(join(workspace, "detached"), "utf8"),
					detached,
					"completed tool leaked a detached descendant",
				);
				assert.equal(
					await readFile(join(workspace, "after-revoke"), "utf8"),
					"fresh epoch",
				);
				const before = await readFile(join(workspace, "heartbeat"), "utf8");
				await sleep(150);
				assert.equal(
					await readFile(join(workspace, "heartbeat"), "utf8"),
					before,
				);
				const entries = (
					await conversation.entries({}, 1000, undefined, context)
				).items;
				assert.ok(!JSON.stringify(entries).includes("composed-secret-1962"));
				assert.ok(
					toolResults(entries).some(
						(m) => m.toolCallId === "active-shell" && m.isError,
					),
				);
			},
			{
				decide: (request, operator) => {
					const control = (async () => {
						await operator.call("decide", {
							session: request.session,
							...request,
							allow: true,
							lifetime: "session",
						});
						if (request.argumentsText.includes("cat private.txt")) {
							for (let i = 0; i < 80; i++) {
								try {
									if ((await readFile(join(shared, "heartbeat"))).length) break;
								} catch {}
								await sleep(25);
							}
							await sleep(300);
							await operator.call("revoke_all", { session: request.session });
						}
					})();
					controls.push(control);
					return control;
				},
			},
		);
	},
);
