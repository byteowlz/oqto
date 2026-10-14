// Copied beside the upstream experimental session worker in the private runtime.
import { join, resolve, sep } from "node:path";
import {
	createStaticFacetLoader,
	defineFacet,
	type MutableReplicatedState,
} from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import {
	type PermissionState,
	Permissions,
} from "../../examples/plugins/oq-permissions/src/contract.ts";
import { ModelRuntime } from "../core/model-runtime.ts";
import { openCodingRuntime } from "./coding-runtime.ts";
import { ownerModelDefaults, ownerModelsStore } from "./owner-models.ts";
import { readOnlyCredentials, readOwnedJson } from "./readonly-profile.ts";
import type { SessionWorkerRuntime } from "./services/worker.ts";
import type { runSessionWorkerWithHarness } from "./session-worker.ts";

type Options = Parameters<Parameters<typeof runSessionWorkerWithHarness>[1]>[1];
function required(name: string) {
	const value = process.env[name];
	if (!value) throw new Error(`Missing protected coding context: ${name}`);
	return value;
}
function rejectCommands(value: unknown) {
	if (typeof value === "string" && value.startsWith("!"))
		throw new Error(
			"Command-valued model configuration is unavailable in oqt; use static owning credentials",
		);
	if (Array.isArray(value)) value.forEach(rejectCommands);
	else if (value && typeof value === "object")
		Object.values(value).forEach(rejectCommands);
}

export async function createCodingHarness(
	databasePath: string,
	options: Options,
): Promise<SessionWorkerRuntime> {
	const owner = required("OQT_OWNER_AGENT_DIR");
	const trusted = required("OQT_TRUSTED_SOURCE_ROOT");
	const workspace = resolve(options.metadata.cwd);
	if (workspace === trusted || workspace.startsWith(trusted + sep))
		throw new Error(
			"Choose a coding workspace outside oqt's own trusted source tree (use --workdir)",
		);
	const defaults = await ownerModelDefaults(owner);
	const modelsPath = join(owner, "models.json");
	try {
		rejectCommands(await readOwnedJson(modelsPath, false));
	} catch (error) {
		if (!(error instanceof Error && "code" in error && error.code === "ENOENT"))
			throw error;
	}
	const modelRuntime = await ModelRuntime.create({
		credentials: readOnlyCredentials(owner),
		modelsPath,
		modelsStore: ownerModelsStore(owner),
		allowModelNetwork: false,
	});
	const model = modelRuntime
		.getAllModels(defaults.provider)
		.find((candidate) => candidate.id === defaults.model);
	if (!model)
		throw new Error(
			`Owning model is unavailable: ${defaults.provider}/${defaults.model}; no default fallback`,
		);
	// Validate auth before opening a session or attempting a provider request.
	const auth = await modelRuntime.getAuth(model);
	if (!auth)
		throw new Error(
			"Owning model credentials unavailable; login/renew through owning Pi",
		);
	const levels = ["off", "minimal", "low", "medium", "high", "xhigh"] as const;
	const thinking = levels.find((level) => level === defaults.thinking);
	if (defaults.thinking !== undefined && thinking === undefined)
		throw new Error("Unsupported owning thinking level");
	let state: MutableReplicatedState<PermissionState> | undefined;
	let pending: PermissionState["pending"] = [];
	const publish = () =>
		state?.change(BACKGROUND_CONTEXT, (draft) => {
			draft.pending = [...pending];
		});
	const runtime = await openCodingRuntime({
		databasePath,
		directory: join(options.metadata.path, "coding-authority"),
		session: options.metadata.id,
		workspace: options.metadata.cwd,
		launcher: required("OQT_HANDS"),
		daemon: required("OQT_ENV_DAEMON"),
		bpf: required("OQT_SECCOMP_BPF"),
		protectedRoots: [
			owner,
			trusted,
			required("OQT_SOURCE_CHECKOUT"),
			required("OQT_RUNTIME_ROOT"),
			required("OQT_SERVER_DIR"),
			options.metadata.path,
		],
		models: modelRuntime,
		model: { provider: model.provider, modelId: model.id },
		thinkingLevel: thinking,
		policy: "ask",
		publish(requests) {
			pending = requests.map((request) => ({
				id: request.id,
				digest: request.digest,
				revision: request.revision,
				capability: request.operation.arguments.tool,
				resource: "confined workspace",
				argumentsText: JSON.stringify(request.operation.arguments.input),
				session: options.metadata.id,
			}));
			publish();
		},
	});
	const facet = defineFacet({
		id: "oq.permissions.coding.provider",
		setup(env) {
			const ownState = env.replicatedState<PermissionState>({
				pending: [],
				mode: "coding",
			});
			env.provide(Permissions, {
				state: ownState,
				async decide(decision, context) {
					context.abortSignal?.throwIfAborted();
					await runtime.authority.operator.call({
						command: "decide",
						...decision,
					});
				},
				async revokeAll(context) {
					context.abortSignal?.throwIfAborted();
					await runtime.authority.revokeAll();
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
	return {
		...runtime,
		modelRuntime,
		facetLoader: createStaticFacetLoader([facet]),
	};
}
