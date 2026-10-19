// Host-neutral worker routing over native providers. No store writes outside Harness.
import { randomUUID } from "node:crypto";
import {
	type AttachedReplicatedState,
	type Context,
	defineFacet,
	type Facet,
	type JsonValue,
	type MutableReplicatedState,
	type RemoteServiceTransport,
	type ServiceProviderUpdate,
	type ServiceSubscriptionSnapshot,
} from "@earendil-works/chord";
import {
	BACKGROUND_CONTEXT,
	createContextKey,
	withContextValue,
} from "@earendil-works/chord/context";
import { applyImmutable } from "@earendil-works/chord/delta";
import {
	AgentDoc,
	type AgentState,
	type Conversation,
	type ConversationId,
	type ConversationView,
	type DocumentState,
	type Harness,
	type SubmissionId,
} from "@earendil-works/pi-durable";
import {
	AgentController,
	type AgentController as Controller,
} from "./services/agent-controller.ts";
import { createAgentController } from "./services/agent-controller-provider.ts";
import {
	Models,
	type Models as ModelsService,
	type ModelsState,
} from "./services/models.ts";
import {
	createModelsService,
	type ModelsServiceRuntime,
} from "./services/models-provider.ts";
import { Transcript } from "./services/transcript.ts";
import { createSessionForks, SessionForks } from "./session-forks.ts";

const TOKEN = "oqtConversationTarget";
const OBSERVED_TARGET = createContextKey<string>(
	"oqt.observedConversationTarget",
);

/** Retain a command's opening view across awaited pickers and plugin callbacks. */
export function captureConversationContext(
	observed: unknown,
	context: Context,
): Context {
	const token = tokenOf(observed);
	return token ? withContextValue(OBSERVED_TARGET, token, context) : context;
}
type Marker = { oqtConversationTarget: string };
type TargetedView = ConversationView & Marker;
type TargetedModels = ModelsState & Marker;

/** Current-view mutations require the caller's observed token. Submission-ID
 * cleanup is a separate identity-targeted action, guarded by host input leases.
 */
export function isConversationMutation(
	serviceId: string,
	member: string,
): boolean {
	return (
		(serviceId === AgentController.id &&
			member !== "waitForPrompt" &&
			member !== "cancelQueued") ||
		(serviceId === Models.id && member !== "getThinkingLevels") ||
		(serviceId === SessionForks.id &&
			(member === "select" || member === "fork"))
	);
}
function tokenOf(value: unknown): string | undefined {
	if (typeof value !== "object" || value === null) return undefined;
	const token = (value as Record<string, unknown>)[TOKEN];
	return typeof token === "string" && token.length > 0 ? token : undefined;
}

/** Capture the view at command/picker OPEN, not at its later completion. */
export function invokeAtConversation<A extends unknown[], R>(
	method: (...args: [...A, Context]) => Promise<R>,
	args: A,
	observed: unknown,
	context: Context,
): Promise<R> {
	const token = tokenOf(observed);
	if (!token)
		return Promise.reject(new Error("Conversation view is not ready"));
	const invoke = method as (...args: unknown[]) => Promise<R>;
	return invoke(...args, { [TOKEN]: token }, context);
}

/** Private pinned client seam. Native/headless transports otherwise stay unchanged.
 * Tokens are taken only after the binding consumed the corresponding view frame,
 * not fetched at invocation time (which would bless a stale client's request).
 */
export function fenceConversationTransport(
	native: RemoteServiceTransport,
): RemoteServiceTransport {
	const observations = new Map<
		object,
		{ serviceId: string; value?: unknown; token?: string }
	>();
	const targetFor = (serviceId: string) => {
		const source = serviceId === Models.id ? Models.id : Transcript.id;
		const tokens = [...observations.values()]
			.filter((item) => item.serviceId === source)
			.map((item) => item.token);
		if (
			tokens.length === 0 ||
			tokens.some((token) => token === undefined || token !== tokens[0])
		)
			throw new Error(
				"Conversation view is not ready or subscriptions disagree; retry after synchronization",
			);
		return tokens[0];
	};
	return {
		invoke(call, context) {
			if (!isConversationMutation(call.serviceId, call.member))
				return native.invoke(call, context);
			// Picker commands may already carry the snapshot they opened against.
			if (tokenOf(call.args.at(-1))) return native.invoke(call, context);
			const token = context.value(OBSERVED_TARGET) ?? targetFor(call.serviceId);
			return native.invoke(
				{ ...call, args: [...call.args, { [TOKEN]: token } as JsonValue] },
				context,
			);
		},
		async subscribe(serviceId, mode, listener, context) {
			if (serviceId !== Transcript.id && serviceId !== Models.id)
				return native.subscribe(serviceId, mode, listener, context);
			const key = {};
			const observation: {
				serviceId: string;
				value?: unknown;
				token?: string;
			} = { serviceId };
			let live = true;
			const snapshot = (baseline: ServiceSubscriptionSnapshot) => {
				const member = baseline.instances[0]?.members.find(
					(member) => member.name === "state" && member.kind === "state",
				);
				observation.value =
					member?.kind === "state"
						? applyImmutable(undefined, member.ops)
						: undefined;
				observation.token = tokenOf(observation.value);
			};
			const update = (frame: ServiceProviderUpdate) => {
				if (frame.type === "reset") snapshot(frame.snapshot);
				else if (frame.type === "replaced")
					snapshot({ serviceId, mode, instances: [frame.snapshot] });
				else if (frame.type === "state" && frame.member === "state") {
					observation.value = applyImmutable(observation.value, frame.ops);
					observation.token = tokenOf(observation.value);
				} else if (frame.type === "unavailable" || frame.type === "closed") {
					observation.value = undefined;
					observation.token = undefined;
				}
			};
			const subscription = await native.subscribe(
				serviceId,
				mode,
				(frame, frameContext) => {
					if (!live) return;
					// Readiness is invalidated during delivery, including reentrant invocations.
					observation.token = undefined;
					listener(frame, frameContext);
					update(frame);
				},
				context,
			);
			return {
				snapshot: subscription.snapshot,
				activate() {
					snapshot(subscription.snapshot);
					observations.set(key, observation);
					subscription.activate();
				},
				close(closeContext) {
					live = false;
					observations.delete(key);
					return subscription.close(closeContext);
				},
			};
		},
	};
}

/** Resolve only inside this worker's Harness. The host's SessionInputLeases
 * authorizes the attachment's exact recorded ID before remote dispatch; private
 * coordinator cleanup uses this same existing method without a view marker.
 * Never route cancellation through the currently selected conversation.
 */
export async function cancelQueuedById(
	harness: Harness,
	value: unknown,
	context: Context,
): ReturnType<Controller["cancelQueued"]> {
	if (typeof value !== "string" || !/^[1-9][0-9]*$/.test(value))
		return { outcome: "not_found" };
	const number = Number(value);
	if (!Number.isSafeInteger(number) || String(number) !== value)
		return { outcome: "not_found" };
	const submission = await harness.submission(number as SubmissionId, context);
	if (!submission) return { outcome: "not_found" };
	const record = await submission.status(context);
	// Input authority never grants cancellation of passive receipt/context writes.
	if (record.type !== "input") return { outcome: "not_found" };
	const original = await harness.conversation(record.conversationId, context);
	if (!original) return { outcome: "not_found" };
	return createAgentController(harness, original).cancelQueued(value, context);
}

interface Binding {
	conversation: Conversation;
	view: AttachedReplicatedState<ConversationView>;
	agent: DocumentState<AgentState>;
	controller: Controller;
	models?: ModelsServiceRuntime;
	modelState?: MutableReplicatedState<ModelsState>;
	token: string;
	off: Array<() => void>;
}

export async function createSessionRouter(
	options: {
		harness: Harness;
		modelRuntime?: Parameters<typeof createModelsService>[2];
		settingsManager?: Parameters<typeof createModelsService>[3];
		/** Pin source and block external effects BEFORE the native selection commit. */
		beginNavigation?: (
			id: ConversationId,
			context: Context,
		) => Promise<unknown>;
		/** Commit/unlock the pinned destination BEFORE publishing the new view. */
		endNavigation?: (
			fence: unknown,
			id: ConversationId,
			context: Context,
		) => Promise<void>;
	},
	context: Context,
): Promise<{
	conversation: Conversation;
	conversationFacet: Facet;
	currentConversationId(): ConversationId;
	close(): void;
}> {
	const { harness } = options;
	if (Boolean(options.beginNavigation) !== Boolean(options.endNavigation))
		throw new Error("Navigation fence requires both begin and end callbacks");
	const acquire = async (
		id: ConversationId,
		ctx: Context,
	): Promise<Binding> => {
		const conversation = await harness.conversation(id, ctx);
		if (!conversation)
			throw new Error(`Selected conversation ${id} is missing`);
		const view = await conversation.viewState(ctx);
		try {
			const agent = await harness.documentState(AgentDoc, id, ctx);
			if (!agent)
				throw new Error(`Selected conversation ${id} has no agent document`);
			return {
				conversation,
				view,
				agent,
				controller: createAgentController(harness, conversation),
				token: randomUUID(),
				off: [],
			};
		} catch (error) {
			view.dispose();
			throw error;
		}
	};
	const dispose = (binding: Binding) => {
		for (const off of binding.off.splice(0)) off();
		binding.view.dispose();
		binding.agent.dispose();
	};
	let active: Binding;
	let closed = false;
	let failed = false;
	let navigation:
		| { source: ConversationId; fence: unknown; committed: boolean }
		| undefined;
	let tail: Promise<unknown> = Promise.resolve();
	let transcript: MutableReplicatedState<TargetedView> | undefined;
	let modelState: MutableReplicatedState<TargetedModels> | undefined;
	let makeState:
		| (<T extends object>(value: T) => MutableReplicatedState<T>)
		| undefined;
	const publish = (binding: Binding, ctx: Context) => {
		if (closed || failed || active !== binding) return;
		transcript?.replace(ctx, { ...binding.view.value, [TOKEN]: binding.token });
		if (binding.modelState)
			modelState?.replace(ctx, {
				...binding.modelState.value,
				[TOKEN]: binding.token,
			});
	};
	const attach = async (binding: Binding, ctx: Context) => {
		const stateFactory = makeState;
		if (!stateFactory) throw new Error("Conversation facet is not active");
		binding.models = createModelsService(
			binding.conversation,
			binding.agent,
			options.modelRuntime,
			options.settingsManager,
			(initial) => {
				const state = stateFactory(initial);
				binding.modelState = state;
				return state;
			},
		);
		await binding.models.activate(ctx);
		binding.off.push(
			binding.view.subscribe((_value, updateContext) =>
				publish(binding, updateContext),
			),
		);
		binding.off.push(
			binding.agent.subscribe((_value, updateContext) => {
				if (!closed && !failed && active === binding)
					binding.models?.syncConfiguration(updateContext);
			}),
		);
		if (!binding.modelState) throw new Error("Native models state missing");
		binding.off.push(
			binding.modelState.subscribe((_value, updateContext) =>
				publish(binding, updateContext),
			),
		);
	};
	const forks = createSessionForks(harness, async (id, ctx) => {
		// Selection has already committed. No further mutation is admitted on failure;
		// reopening reacquires precisely that durable selection, never silently root.
		let candidate: Binding | undefined;
		try {
			if (!navigation)
				throw new Error("Native navigation has no admission fence");
			navigation.committed = true;
			candidate = await acquire(id, ctx);
			await attach(candidate, ctx);
			await options.endNavigation?.(navigation.fence, id, ctx);
			const previous = active;
			active = candidate;
			dispose(previous);
			publish(candidate, ctx);
		} catch (error) {
			if (candidate && candidate !== active) dispose(candidate);
			failed = true;
			throw error;
		}
	});
	active = await acquire(await forks.selected(context), context);
	const serialize = <T>(change: () => Promise<T>): Promise<T> => {
		const operation = tail.then(change);
		tail = operation.catch(() => {});
		return operation;
	};
	const run = <T>(
		marker: unknown,
		ctx: Context,
		change: (binding: Binding) => Promise<T>,
		navigate = false,
	): Promise<T> => {
		const expected = tokenOf(marker);
		return serialize(async () => {
			ctx.abortSignal?.throwIfAborted();
			if (closed || failed)
				throw new Error(
					"Conversation router unavailable; reconnect to the worker",
				);
			if (!expected || expected !== active.token)
				throw new Error(
					"Stale conversation target; refresh the view before retrying",
				);
			if (navigate) {
				const inspection = await harness.inspect(ctx);
				if (inspection.tasks.length || inspection.submissions.length)
					throw new Error(
						"Cannot navigate while native tasks or queued submissions are active",
					);
			}
			if (!navigate) return change(active);
			const source = active.conversation.id;
			const fence = await options.beginNavigation?.(source, ctx);
			const attempt = { source, fence, committed: false };
			navigation = attempt;
			try {
				return await change(active);
			} catch (error) {
				// A cancelled native call can have committed before its waiter failed.
				// Consult durable identity, never guess from callback completion.
				const selected = await forks
					.selected(BACKGROUND_CONTEXT)
					.catch(() => undefined);
				if (attempt.committed || selected !== source) failed = true;
				else {
					try {
						await options.endNavigation?.(fence, source, BACKGROUND_CONTEXT);
					} catch {
						failed = true;
					}
				}
				throw error;
			} finally {
				navigation = undefined;
			}
		});
	};
	/** Chord appends Context after JSON arguments. The transport adds one marker.
	 * These are our service adapters, not modifications of native provider methods.
	 */
	type Method = (...args: never[]) => Promise<unknown>;
	type MethodArgs<F> = F extends (
		...args: [...infer A, Context]
	) => Promise<unknown>
		? A
		: never;
	const routed = <F extends Method>(
		invoke: (
			binding: Binding,
			args: MethodArgs<F>,
			ctx: Context,
		) => ReturnType<F>,
		navigate = false,
	): F =>
		((...values: unknown[]) => {
			const ctx = values.pop() as Context;
			const marker = values.pop();
			return run(
				marker,
				ctx,
				(binding) => invoke(binding, values as MethodArgs<F>, ctx),
				navigate,
			);
		}) as unknown as F;
	const controller: Controller = {
		prompt: routed<Controller["prompt"]>((binding, [request], ctx) =>
			binding.controller.prompt(request, ctx),
		),
		steer: routed<Controller["steer"]>((binding, [request], ctx) =>
			binding.controller.steer(request, ctx),
		),
		followUp: routed<Controller["followUp"]>((binding, [request], ctx) =>
			binding.controller.followUp(request, ctx),
		),
		cancelQueued: (id, ctx) =>
			serialize(async () => {
				ctx.abortSignal?.throwIfAborted();
				if (closed) throw new Error("Conversation router is closed");
				// Even a failed rebind must allow identity-scoped cleanup. It cannot
				// admit new input or mutate an accidentally selected/latest target.
				return cancelQueuedById(harness, id, ctx);
			}),
		abort: routed<Controller["abort"]>((binding, _args, ctx) =>
			binding.controller.abort(ctx),
		),
		compact: routed<Controller["compact"]>((binding, [request], ctx) =>
			binding.controller.compact(request, ctx),
		),
		// Native operation IDs retain their identity even after navigation.
		waitForPrompt: (id, ctx) => active.controller.waitForPrompt(id, ctx),
	};
	const nativeModels = (binding: Binding) => {
		if (!binding.models) throw new Error("Native models provider not active");
		return binding.models.service;
	};
	const modelMethods: Omit<ModelsService, "state"> = {
		cycleThinking: routed<ModelsService["cycleThinking"]>(
			(binding, _args, ctx) => nativeModels(binding).cycleThinking(ctx),
		),
		selectThinking: routed<ModelsService["selectThinking"]>(
			(binding, [level], ctx) =>
				nativeModels(binding).selectThinking(level, ctx),
		),
		select: routed<ModelsService["select"]>((binding, [model], ctx) =>
			nativeModels(binding).select(model, ctx),
		),
		refresh: routed<ModelsService["refresh"]>((binding, _args, ctx) =>
			nativeModels(binding).refresh(ctx),
		),
		getThinkingLevels: (ctx) => nativeModels(active).getThinkingLevels(ctx),
	};
	const navigationService: SessionForks = {
		selected: (ctx) => forks.selected(ctx),
		conversations: (limit, cursor, ctx) =>
			forks.conversations(limit, cursor, ctx),
		entries: (id, limit, cursor, ctx) => forks.entries(id, limit, cursor, ctx),
		select: routed<SessionForks["select"]>(
			(_binding, [id], ctx) => forks.select(id, ctx),
			true,
		),
		fork: routed<SessionForks["fork"]>((binding, [id, at], ctx) => {
			if (binding.conversation.id !== id)
				throw new Error("Fork parent is not the selected conversation");
			return forks.fork(id, at, ctx);
		}, true),
	};
	const close = () => {
		if (!closed) {
			closed = true;
			dispose(active);
		}
	};
	const conversationFacet = defineFacet({
		id: "oqt.conversation-router",
		setup(env) {
			makeState = env.replicatedState;
			transcript = env.replicatedState({
				...active.view.value,
				[TOKEN]: active.token,
			});
			// Stable outward identities survive every navigation. Native provider state
			// remains private and is mirrored only while its binding is still current.
			modelState = env.replicatedState<TargetedModels>({
				catalog: { revision: 0, availableModels: [] },
				configuration: { model: null, thinkingLevel: "off" },
				refresh: { status: "idle" },
				[TOKEN]: active.token,
			});
			env.provide(AgentController, controller);
			env.provide(Models, { ...modelMethods, state: modelState });
			env.provide(Transcript, { state: transcript });
			env.provide(SessionForks, navigationService);
			env.own(close);
			env.onActivate(async () => {
				await attach(active, BACKGROUND_CONTEXT);
				publish(active, BACKGROUND_CONTEXT);
			});
		},
	});
	return {
		conversation: active.conversation,
		conversationFacet,
		currentConversationId: () => active.conversation.id,
		close,
	};
}
