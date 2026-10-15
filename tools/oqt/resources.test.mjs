import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { join } from "node:path";
import { test } from "node:test";
import {
	call,
	fauxAssistantMessage,
	toolResults,
	turn,
	withGuard,
} from "./guard-fixture.mjs";

const decideAll = async (request, operator) =>
	operator.call("decide", {
		session: request.session,
		...request,
		allow: true,
		lifetime: request.kind === "resource" ? "session" : "once",
	});
// Created before the guard opens so the ceiling can name it.
const ALLOW_ROOT = await mkdtemp("/var/tmp/oqt-allow-");
const results = (entries) =>
	Object.fromEntries(toolResults(entries).map((m) => [m.toolCallId, m]));
const text = (message) => JSON.stringify(message?.content ?? "");

test(
	"approved outside path becomes a read-only mount on fresh hands; revoke removes it",
	{ timeout: 30000 },
	async () => {
		await withGuard(
			async ({
				root,
				faux,
				conversation,
				requests,
				operator,
				session,
				guardSession,
			}) => {
				const outside = join(root, "outside");
				await mkdir(outside);
				await writeFile(join(outside, "data.txt"), "outside-data-4417");
				faux.setResponses([
					call("read", { path: join(outside, "data.txt") }, "before-grant"),
					call(
						"request_access",
						{
							path: outside,
							access: "read",
							reason: "compare with the shared library",
						},
						"ask-read",
					),
					call("read", { path: join(outside, "data.txt") }, "after-grant"),
					call(
						"write",
						{ path: join(outside, "new.txt"), content: "x" },
						"write-ro",
					),
					fauxAssistantMessage("done"),
				]);
				let entries = results(await turn(conversation, "grant-turn"));
				assert.ok(entries["before-grant"].isError, "invisible before approval");
				assert.ok(!text(entries["before-grant"]).includes("outside-data-4417"));
				assert.ok(!entries["ask-read"].isError, text(entries["ask-read"]));
				console.error(
					"AFTER-GRANT:",
					text(entries["after-grant"]).slice(0, 300),
				);
				assert.ok(text(entries["after-grant"]).includes("outside-data-4417"));
				assert.ok(
					entries["write-ro"].isError,
					"read grant is not a write grant",
				);
				await assert.rejects(readFile(join(outside, "new.txt")), {
					code: "ENOENT",
				});
				const resource = requests.find((r) => r.kind === "resource");
				assert.equal(resource.capability, "read");
				assert.equal(resource.resource, outside);
				assert.equal(resource.argumentsText, "compare with the shared library");
				assert.deepEqual(
					guardSession.state().resources.map((r) => [r.capability, r.resource]),
					[["read", outside]],
				);
				await operator.call("revoke_all", { session });
				assert.deepEqual(guardSession.state().resources, []);
				faux.setResponses([
					call("read", { path: join(outside, "data.txt") }, "after-revoke"),
					fauxAssistantMessage("done"),
				]);
				entries = results(await turn(conversation, "revoked-turn"));
				assert.ok(
					entries["after-revoke"].isError,
					"mount removed after revoke",
				);
				assert.ok(!text(entries["after-revoke"]).includes("outside-data-4417"));
			},
			{ decide: decideAll },
		);
	},
);

test(
	"write grants and network grants reach the hands only after approval",
	{ timeout: 30000 },
	async () => {
		const server = createServer((socket) => {
			socket.on("error", () => {});
			socket.end("network-reply-9921\n");
		});
		await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
		const port = server.address().port;
		const probe = `exec 3<>/dev/tcp/127.0.0.1/${port} && cat <&3`;
		try {
			await withGuard(
				async ({ root, faux, conversation }) => {
					const shared = join(root, "shared");
					await mkdir(shared);
					faux.setResponses([
						call("bash", { command: probe }, "net-before"),
						call(
							"request_access",
							{ network: true, reason: "fetch a dependency" },
							"ask-net",
						),
						call("bash", { command: probe }, "net-after"),
						call(
							"request_access",
							{ path: shared, access: "write", reason: "write the report" },
							"ask-write",
						),
						call(
							"write",
							{ path: join(shared, "report.txt"), content: "report" },
							"write-ok",
						),
						fauxAssistantMessage("done"),
					]);
					const entries = results(await turn(conversation));
					assert.ok(
						!text(entries["net-before"]).includes("network-reply-9921"),
					);
					assert.ok(text(entries["net-after"]).includes("network-reply-9921"));
					assert.ok(!entries["write-ok"].isError, text(entries["write-ok"]));
					assert.equal(
						await readFile(join(shared, "report.txt"), "utf8"),
						"report",
					);
				},
				{ decide: decideAll },
			);
		} finally {
			server.close();
		}
	},
);

test(
	"hard denies and narrowing ceilings refuse requests before any operator prompt",
	{ timeout: 30000 },
	async () => {
		await withGuard(
			async ({ root, faux, conversation, requests }) => {
				const outside = join(root, "outside");
				await mkdir(outside);
				faux.setResponses([
					call(
						"request_access",
						{ path: join(root, "owner"), access: "read", reason: "x" },
						"owner",
					),
					call(
						"request_access",
						{ path: root, access: "read", reason: "x" },
						"container",
					),
					call(
						"request_access",
						{ path: "/etc/shadow", access: "read", reason: "x" },
						"system",
					),
					call("request_access", { network: true, reason: "x" }, "net"),
					call(
						"request_access",
						{ path: outside, access: "read", reason: "x" },
						"allowed",
					),
					fauxAssistantMessage("done"),
				]);
				const entries = results(await turn(conversation));
				for (const id of ["owner", "container", "system", "net"])
					assert.ok(entries[id].isError, `${id} must be refused`);
				// Only the requestable path reached the operator.
				assert.deepEqual(
					requests.map((r) => r.resource),
					[outside],
				);
			},
			{ decide: decideAll, ceiling: 'network = "deny"' },
		);
		// The workspace file may narrow further (and cannot widen).
		await withGuard(
			async ({ root, faux, conversation, requests }) => {
				const outside = join(root, "outside");
				await mkdir(outside);
				faux.setResponses([
					call(
						"request_access",
						{ path: outside, access: "read", reason: "x" },
						"narrowed",
					),
					fauxAssistantMessage("done"),
				]);
				const entries = results(await turn(conversation));
				assert.ok(entries.narrowed.isError);
				assert.equal(requests.length, 0);
			},
			{ decide: decideAll, workspaceCeiling: 'outside = "deny"' },
		);
	},
);

test(
	"global allow_paths pre-approves exact directories: no prompt, no grant, still denied per-path",
	{ timeout: 30000 },
	async () => {
		assert(typeof ALLOW_ROOT === "string");
		await withGuard(
			async ({ faux, conversation, requests, guardSession }) => {
				const outside = join(ALLOW_ROOT, "outside");
				await mkdir(outside, { recursive: true });
				await writeFile(join(outside, "data.txt"), "preapproved-3311");
				await mkdir(join(outside, "secret"), { recursive: true });
				await writeFile(join(outside, "secret", "key"), "denied-secret-88");
				faux.setResponses([
					call("read", { path: join(outside, "data.txt") }, "allowed"),
					call("read", { path: join(outside, "secret/key") }, "denied"),
					fauxAssistantMessage("done"),
				]);
				const entries = results(await turn(conversation, "allow-turn"));
				assert.ok(!entries.allowed.isError, text(entries.allowed));
				assert.ok(text(entries.allowed).includes("preapproved-3311"));
				// The masked path fails closed; its content is never disclosed.
				assert.ok(!text(entries.denied).includes("denied-secret-88"));
				assert.deepEqual(
					requests.filter((r) => r.kind === "resource"),
					[],
					"no access request needed for pre-approved paths",
				);
				assert.deepEqual(guardSession.state().resources, [], "no grant needed");
			},
			{
				ceiling: `policy = { "${ALLOW_ROOT}/**" = "auto", "${ALLOW_ROOT}/outside/secret" = "deny" }`,
			},
		);
	},
);
