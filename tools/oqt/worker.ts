// Copied into a private, pinned source runtime; never installed into ordinary Pi.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, mkdir, writeFile } from "node:fs/promises";
import { userInfo } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout as sleep } from "node:timers/promises";
import {
	type Context,
	createStaticFacetLoader,
	defineFacet,
	type MutableReplicatedState,
} from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import {
	createModels,
	fauxAssistantMessage,
	fauxProvider,
	fauxToolCall,
	type TranscriptContext,
	Type,
} from "@earendil-works/pi-ai";
import {
	createRegistry,
	defineExtension,
	defineTool,
	Harness,
	type ToolExecutionApi,
	type ToolExecutionResult,
} from "@earendil-works/pi-durable";
import { openNodeSqliteStorage } from "@earendil-works/pi-durable/storage/sqlite/node";
import type { Static } from "typebox";
import { Check } from "typebox/value";
import {
	type PendingPermission,
	type PermissionState,
	Permissions,
} from "../../examples/plugins/oq-permissions/src/contract.ts";
import type { SessionWorkerRuntime } from "./services/worker.ts";
import type { runSessionWorkerWithHarness } from "./session-worker.ts";

type WorkerFactory = Parameters<typeof runSessionWorkerWithHarness>[1];
type SessionWorkerOptions = Parameters<WorkerFactory>[1];

const PendingSchema = Type.Object({
	digest: Type.String(),
	revision: Type.Integer(),
	operation: Type.Object({
		target: Type.Object({
			capability: Type.String(),
			resource_id: Type.String(),
		}),
		arguments: Type.Unknown(),
	}),
});
const InvokeSchema = Type.Union([
	Type.Object({ status: Type.Literal("allowed"), text: Type.String() }),
	Type.Object({
		admission: Type.Union([
			Type.Object({ status: Type.Literal("pending"), value: PendingSchema }),
			Type.Object({ status: Type.Literal("denied") }),
		]),
	}),
]);
function invokeResult(value: unknown): Static<typeof InvokeSchema> {
	if (!Check(InvokeSchema, value))
		throw new Error("Invalid permission-carrier response");
	return value;
}
function record(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null;
}

class Carrier {
	readonly child;
	readonly replies: Array<{
		resolve: (value: unknown) => void;
		reject: (error: Error) => void;
		timer: ReturnType<typeof setTimeout>;
	}> = [];
	constructor(role: string, directory: string, binding: object) {
		const binary = process.env.OQT_CARRIER;
		if (!binary)
			throw new Error("Missing protected demo carrier; refusing fallback");
		this.child = spawn(binary, [role, directory], {
			stdio: ["pipe", "pipe", "inherit"],
			env: {
				PATH: process.env.PATH,
				OQ_ADAPTIVE_SECCOMP_BPF: process.env.OQT_SECCOMP_BPF,
				OQTO_SANDBOX_SHIM_BIN: process.env.OQT_SANDBOX_SHIM,
				OQ_DEMO_BINDING: JSON.stringify(binding),
			},
		});
		const lines = createInterface({ input: this.child.stdout });
		lines.on("line", (line) => {
			const call = this.replies.shift();
			if (!call) return;
			clearTimeout(call.timer);
			try {
				const value: unknown = JSON.parse(line);
				if (!record(value) || typeof value.ok !== "boolean")
					throw new Error("Invalid carrier envelope");
				if (!value.ok) call.reject(new Error(String(value.error)));
				else call.resolve(value.result);
			} catch (error) {
				call.reject(new Error(String(error)));
			}
		});
		this.child.on("error", (error) => this.rejectAll(error));
		this.child.on("exit", () => {
			lines.close();
			this.rejectAll(new Error("Permission carrier disconnected"));
		});
	}
	rejectAll(error: Error) {
		for (const call of this.replies.splice(0)) {
			clearTimeout(call.timer);
			call.reject(error);
		}
	}
	call(command: object): Promise<unknown> {
		return new Promise((resolve, reject) => {
			const timer = setTimeout(() => {
				this.child.kill();
				reject(new Error("Carrier deadline"));
			}, 5000);
			this.replies.push({ resolve, reject, timer });
			this.child.stdin.write(`${JSON.stringify(command)}\n`);
		});
	}
	async close() {
		if (this.child.exitCode !== null || this.child.signalCode !== null) return;
		this.child.stdin.end();
		await new Promise<void>((resolve) => {
			const timer = setTimeout(() => this.child.kill("SIGKILL"), 1000);
			this.child.once("exit", () => {
				clearTimeout(timer);
				resolve();
			});
		});
	}
}

export async function createOqtHarness(
	databasePath: string,
	options: SessionWorkerOptions,
): Promise<SessionWorkerRuntime> {
	if (process.env.OQT_MODE === "coding")
		return (await import("./coding-worker.ts")).createCodingHarness(
			databasePath,
			options,
		);
	if (process.env.OQT_MODE && process.env.OQT_MODE !== "demo")
		throw new Error("Unsupported oqt mode; refusing fallback");
	const directory = join(options.metadata.path, "permissions-demo");
	await mkdir(directory, { recursive: true, mode: 0o700 });
	await chmod(directory, 0o700);
	await mkdir(join(directory, "workdir"), { recursive: true });
	await mkdir(join(directory, "extra"), { recursive: true });
	await writeFile(
		join(directory, "extra/token"),
		"Real confined hands returned this DEMO fixture-token.\n",
		{ mode: 0o600 },
	);
	const workdirHash = createHash("sha256")
		.update(options.metadata.cwd)
		.digest("hex");
	const binding = {
		account_id: "local-demo-owner",
		principal_id: `uid:${userInfo().uid}`,
		workspace_id: `demo:${workdirHash}`,
		work_directory_id: `workdir:${workdirHash}`,
		session_id: options.metadata.id,
		audience: "local-demo-owner-only",
	};
	const tools = new Carrier("tools", directory, binding),
		operator = new Carrier("operator", directory, binding);
	const pending = new Map<string, PendingPermission>();
	let state: MutableReplicatedState<PermissionState> | undefined;
	const publish = () =>
		state?.change(BACKGROUND_CONTEXT, (draft) => {
			draft.pending = Array.from(pending.values());
		});
	const facet = defineFacet({
		id: "oq.permissions.demo.provider",
		setup(env) {
			const ownState = env.replicatedState<PermissionState>({ pending: [] });
			env.provide(Permissions, {
				state: ownState,
				async decide(decision, context) {
					context.abortSignal?.throwIfAborted();
					await operator.call({
						command: "decide",
						id: decision.id,
						digest: decision.digest,
						revision: decision.revision,
						lifetime: decision.lifetime,
						allow: decision.allow,
					});
					pending.delete(decision.id);
					publish();
				},
				async revokeAll(context) {
					context.abortSignal?.throwIfAborted();
					await operator.call({ command: "revoke_all" });
				},
			});
			env.onActivate(() => {
				state = ownState;
				publish();
			});
			env.onDeactivate(() => {
				if (state === ownState) state = undefined;
			});
		},
	});
	const registry = createRegistry();
	const gated = async (
		target: object,
		args: object,
		api: ToolExecutionApi,
		context: Context,
	): Promise<ToolExecutionResult> => {
		const operation = {
			request_id: `${options.metadata.id}:store:${api.conversationId}:${api.taskId}`,
			target,
			arguments: args,
		};
		try {
			for (;;) {
				context.abortSignal?.throwIfAborted();
				const result = invokeResult(
					await tools.call({ command: "invoke", operation }),
				);
				if ("status" in result && result.status === "allowed")
					return { output: [{ type: "text", text: result.text }] };
				if (!("admission" in result))
					throw new Error("Unexpected carrier response");
				if (result.admission.status === "denied")
					throw new Error("Permission denied or revoked");
				const request = result.admission.value;
				if (!pending.has(operation.request_id)) {
					pending.set(operation.request_id, {
						id: operation.request_id,
						digest: request.digest,
						revision: request.revision,
						capability: request.operation.target.capability,
						resource: request.operation.target.resource_id,
						argumentsText:
							JSON.stringify(request.operation.arguments) ?? "null",
						session: options.metadata.id,
					});
					publish();
				}
				await sleep(250, undefined, { signal: context.abortSignal });
			}
		} finally {
			pending.delete(operation.request_id);
			publish();
		}
	};
	registry.install(
		defineExtension({
			name: "oq-demo-gated-tools",
			tools: [
				defineTool({
					name: "history_search",
					description:
						"DEMO fixture history; broader history requires permission",
					parameters: Type.Object({
						scope: Type.Union([Type.Literal("own"), Type.Literal("account")]),
					}),
					replay: "safe",
					execute: (args, api, context) =>
						gated(
							{
								capability: "history_search",
								resource_id:
									args.scope === "own" ? "own-history" : "account-history",
							},
							{ query: "fixture-token" },
							api,
							context,
						),
				}),
				defineTool({
					name: "file_read",
					description: "DEMO registered file through real confined hands",
					parameters: Type.Object({}),
					replay: "safe",
					execute: (_args, api, context) =>
						gated(
							{ capability: "file_read", resource_id: "extra-resource" },
							{ relative_path: "token" },
							api,
							context,
						),
				}),
			],
		}),
	);
	const faux = fauxProvider();
	faux.setResponses(
		Array.from({ length: 4096 }, () => (input: TranscriptContext) => {
			const last = input.messages.at(-1);
			if (last?.role === "toolResult")
				return fauxAssistantMessage(
					last.content
						.map((part) => (part.type === "text" ? part.text : ""))
						.join("\n"),
				);
			const user = [...input.messages]
				.reverse()
				.find((message) => message.role === "user");
			const text =
				typeof user?.content === "string"
					? user.content
					: JSON.stringify(user?.content ?? "");
			if (/history/i.test(text))
				return fauxAssistantMessage(
					[
						fauxToolCall("history_search", {
							scope: /global|account|broader/i.test(text) ? "account" : "own",
						}),
					],
					{ stopReason: "toolUse" },
				);
			if (/file/i.test(text))
				return fauxAssistantMessage([fauxToolCall("file_read", {})], {
					stopReason: "toolUse",
				});
			return fauxAssistantMessage(
				"oqt DEMO: this is Pi's real Durable TUI with a synthetic model, not a coding session yet. Try 'own history', 'global history', or 'read file'. /permissions revoke clears live approved overlays. No ordinary local tools or credentials are loaded.",
			);
		}),
	);
	const models = createModels();
	models.setProvider(faux.provider);
	try {
		const harness = await Harness.open(
			await openNodeSqliteStorage(databasePath),
			{ models, registry },
			BACKGROUND_CONTEXT,
		);
		const model = faux.getModel();
		const conversation = await harness.root(BACKGROUND_CONTEXT, {
			agent: {
				cwd: options.metadata.cwd,
				model: { provider: model.provider, modelId: model.id },
			},
		});
		return {
			harness,
			conversation,
			facetLoader: createStaticFacetLoader([facet]),
			cleanup: async () => {
				await tools.close();
				await operator.close();
			},
		};
	} catch (error) {
		await tools.close();
		await operator.close();
		throw error;
	}
}
