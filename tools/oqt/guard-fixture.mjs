// Test fixture: a real Guard session (ledger carriers, guard-owned confined
// hands, tool + operator sockets) and the real brain-side composition talking
// to it over those sockets. Only the model provider is synthetic.
import {
	chmod,
	copyFile,
	mkdir,
	mkdtemp,
	rm,
	writeFile,
} from "node:fs/promises";
import { createRequire } from "node:module";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { GuardGate } from "./brain-client.ts";
import { openCodingHarness } from "./coding-runtime.ts";
import { Guard } from "./guard.ts";
import { WireClient } from "./guard-wire.ts";
import { HandsClient } from "./hands-client.ts";
import { installOperatorReceipts } from "./operator-receipts.ts";
import { OwnerBroker } from "./owner-broker.ts";

const require = createRequire(join(process.env.OQT_PI_SOURCE, "package.json"));
export const load = (name) => import(pathToFileURL(require.resolve(name)));
const { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } =
	await load("@earendil-works/pi-ai");
export const { BACKGROUND_CONTEXT: context } = await load(
	"@earendil-works/chord/context",
);
export { fauxAssistantMessage };
export const call = (name, args, id) =>
	fauxAssistantMessage([fauxToolCall(name, args, { id })], {
		stopReason: "toolUse",
	});

/** `decide(request, operator)` runs for each newly pending request. */
export async function withGuard(
	run,
	{ policy = "ask", ceiling, workspaceCeiling, decide, rules = false } = {},
) {
	const root = await mkdtemp("/var/tmp/oqt-guard-proof-");
	await chmod(root, 0o700);
	const workspace = join(root, "workspace");
	const owner = join(root, "owner");
	await mkdir(workspace);
	await mkdir(owner, { mode: 0o700 });
	// One sandbox config (ADR-0060): `ceiling` and `workspaceCeiling` are
	// `[guard]` bodies of the owner's and the workspace's sandbox.toml.
	if (workspaceCeiling) {
		await mkdir(join(workspace, ".oqto"));
		await writeFile(
			join(workspace, ".oqto", "sandbox.toml"),
			`[guard]\n${workspaceCeiling}\n`,
		);
	}
	const userConfig = join(root, "sandbox.toml");
	const ownerGuard =
		ceiling ??
		(policy === "deny" ? 'read = "deny"\nedit = "deny"\nshell = "deny"' : null);
	if (ownerGuard !== null)
		await writeFile(userConfig, `[guard]\n${ownerGuard}\n`);
	const daemon = join(root, "pi-env");
	await copyFile(process.env.OQT_ENV_DAEMON, daemon);
	const guard = await Guard.open({
		authorityRoot: join(root, "authority"),
		...(rules ? { rulesPath: join(root, "rules.json") } : {}),
		protectedRoots: [owner],
		hands: {
			launcher: process.env.OQT_HANDS,
			daemon,
			bpf: process.env.OQT_SECCOMP_BPF,
		},
		broker: new OwnerBroker(owner),
		ceiling: { userConfig, home: root, hardDeny: [] },
		parent: root,
	});
	const session = "coding-public-session";
	const guardSession = guard.session(session, workspace);
	await guardSession.ready;
	const operator = await WireClient.connect(guard.operatorPath);
	const requests = [];
	const seen = new Set();
	const decisions = [];
	let onRequest =
		decide ??
		((request) =>
			operator.call("decide", {
				session,
				...request,
				allow: true,
				lifetime: "once",
			}));
	operator.on("state", (state) => {
		for (const row of state.sessions.flatMap((s) => s.pending))
			if (!seen.has(row.id)) {
				seen.add(row.id);
				requests.push(row);
				decisions.push(Promise.resolve().then(() => onRequest(row, operator)));
			}
	});
	await operator.call("subscribe");
	const tool = await WireClient.connect(guardSession.toolPath);
	const hands = new HandsClient(guardSession.handsPath, workspace);
	const gate = new GuardGate(tool, hands);
	const faux = fauxProvider();
	const models = createModels();
	models.setProvider(faux.provider);
	let runtime;
	let receipts;
	try {
		runtime = await openCodingHarness({
			databasePath: join(root, "native.sqlite"),
			session,
			workspace,
			models,
			model: { provider: "faux", modelId: "faux-1" },
			gate,
			hands,
			refresh: () => gate.refresh(),
			requestAccess: (params, signal) => gate.requestAccess(params, signal),
		});
		receipts = await installOperatorReceipts(
			{
				...runtime,
				session,
				guard: tool,
				selectedConversationId: () => runtime.conversation.id,
			},
			context,
		);
		await run({
			root,
			workspace,
			guard,
			guardSession,
			operator,
			tool,
			faux,
			harness: runtime.harness,
			operatorResults: receipts,
			conversation: runtime.conversation,
			requests,
			session,
			setDecision: (handler) => {
				onRequest = handler;
			},
		});
		await Promise.all(decisions);
	} finally {
		await receipts?.close();
		await runtime?.harness.close(context);
		hands.close();
		tool.close();
		operator.close();
		await guard.close();
		await rm(root, { recursive: true, force: true });
	}
}

export async function turn(conversation, requestId = "gated-turn") {
	const handle = await conversation.submit(
		{ type: "input", content: "code", requestId },
		context,
	);
	const settled = await handle.wait(context);
	if (settled.status !== "done") throw new Error(`turn ${settled.status}`);
	return (await conversation.entries({}, 1000, undefined, context)).items;
}

export const toolResults = (entries) =>
	entries
		.flatMap((entry) => entry.model ?? [])
		.filter((message) => message.role === "toolResult");
