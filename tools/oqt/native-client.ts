// Copied beside experimental/client-runtime.ts. Never use openClientRuntime:
// its discovery/activation behavior is deliberately outside this adapter.
import { basename, isAbsolute } from "node:path";
import {
	BACKGROUND_CONTEXT,
	withAbortSignal,
} from "@earendil-works/chord/context";
import { Client } from "@earendil-works/pi-client";
import { createUnixTransportFactory } from "@earendil-works/pi-client/unix";
import { isServerId } from "@earendil-works/pi-protocol";
import { WireClient } from "./guard-wire.ts";
import { AgentController } from "./services/agent-controller.ts";
import {
	createServerServiceSource,
	createSessionServiceSource,
} from "./services/connection.ts";
import { SessionManagement } from "./services/sessions.ts";

export type NativeAction =
	| "attach"
	| "observe"
	| "request-input"
	| "prompt"
	| "steer"
	| "abort"
	| "release";
export interface NativeBinding {
	readonly socket: string;
	readonly guardSocket: string;
	readonly serverId: string;
	readonly session: string;
	readonly workspace: string;
}
export interface NativeClientOptions extends NativeBinding {
	readonly signal: AbortSignal;
	/** Mandatory Account/MFA/host policy check, NOT local ownership or lease.
	 * Must return literal true for each operation; revocation aborts signal. */
	readonly admit: (
		binding: Readonly<NativeBinding>,
		action: NativeAction,
		signal: AbortSignal,
	) => Promise<boolean>;
	readonly deadlineMs?: number;
}
export class NativeClientError extends Error {
	readonly effect: "not-dispatched" | "uncertain";
	constructor(message: string, effect: "not-dispatched" | "uncertain") {
		super(message);
		this.effect = effect;
	}
}
export interface NativeSessionClient {
	requestInput(label: string): Promise<unknown>;
	prompt(message: string): Promise<unknown>;
	steer(message: string): Promise<unknown>;
	abort(): Promise<unknown>;
	/** Raw SDK snapshot/update values; not a durable history authority. */
	observe(
		listener: (event: { kind: "snapshot" | "update"; value: unknown }) => void,
	): Promise<void>;
	release(): Promise<void>;
	dispose(): Promise<void>;
}

export async function attachNativeSession(
	options: NativeClientOptions,
): Promise<NativeSessionClient> {
	const binding = Object.freeze({
		socket: options.socket,
		guardSocket: options.guardSocket,
		serverId: options.serverId,
		session: options.session,
		workspace: options.workspace,
	});
	const admit = options.admit;
	const external = options.signal;
	const ms = options.deadlineMs ?? 10_000;
	if (
		!isAbsolute(binding.socket) ||
		!isAbsolute(binding.guardSocket) ||
		!isAbsolute(binding.workspace) ||
		!isServerId(binding.serverId) ||
		basename(binding.socket) !== `${binding.serverId}.sock` ||
		!binding.session ||
		binding.session.length > 4096 ||
		typeof admit !== "function" ||
		!external ||
		!Number.isInteger(ms) ||
		ms < 1 ||
		ms > 30_000
	) {
		throw new NativeClientError(
			"Invalid explicit native binding or admission contract",
			"not-dispatched",
		);
	}
	const life = new AbortController();
	const context = withAbortSignal(life.signal, BACKGROUND_CONTEXT);
	let client: Client | undefined;
	let guard: WireClient | undefined;
	let attachmentId: string | undefined;
	let busy = false;
	let observing = false;
	let lease = false;
	let disposed = false;
	const sources: Array<{
		dispose(context: typeof BACKGROUND_CONTEXT): Promise<void>;
	}> = [];
	const dispose = async () => {
		if (disposed) return;
		disposed = true;
		life.abort();
		external.removeEventListener("abort", cancel);
		// Closing this exact Guard peer revokes/drains its lease and pending request.
		guard?.socket.destroy();
		await client?.dispose();
		// No network waits on cleanup: the client has already disconnected.
		await Promise.allSettled(
			sources.map((source) => source.dispose(BACKGROUND_CONTEXT)),
		);
	};
	const cancel = () => {
		void dispose();
	};
	external.addEventListener("abort", cancel, { once: true });
	if (external.aborted) await dispose();
	function current() {
		if (disposed || life.signal.aborted)
			throw new NativeClientError("Native attachment closed", "not-dispatched");
		if (
			attachmentId !== undefined &&
			(!client?.connected ||
				client.attachment?.sessionId !== binding.session ||
				client.attachment.attachmentId !== attachmentId)
		) {
			void dispose();
			throw new NativeClientError(
				"Native attachment binding changed",
				"not-dispatched",
			);
		}
	}
	async function bounded<T>(
		work: () => Promise<T>,
		dispatched: () => boolean,
	): Promise<T> {
		current();
		let timer: ReturnType<typeof setTimeout> | undefined;
		let abort: (() => void) | undefined;
		try {
			return await Promise.race([
				Promise.resolve().then(work),
				new Promise<never>((_, reject) => {
					abort = () => reject(new Error("Native operation cancelled"));
					life.signal.addEventListener("abort", abort, { once: true });
					if (life.signal.aborted) abort();
					timer = setTimeout(
						() => reject(new Error("Native operation deadline exceeded")),
						ms,
					);
				}),
			]);
		} catch (error) {
			await dispose();
			throw new NativeClientError(
				error instanceof Error ? error.message : String(error),
				dispatched() ? "uncertain" : "not-dispatched",
			);
		} finally {
			clearTimeout(timer);
			if (abort) life.signal.removeEventListener("abort", abort);
		}
	}
	async function admitted(action: NativeAction) {
		current();
		if ((await admit(binding, action, life.signal)) !== true)
			throw new Error("Host admission denied");
		current();
	}
	function guardClient() {
		current();
		if (!guard) throw new Error("Guard unavailable");
		return guard;
	}
	async function describe() {
		const value = await guardClient().call(
			"input_describe",
			{ session: binding.session },
			life.signal,
		);
		current();
		if (
			!value ||
			typeof value !== "object" ||
			!("protocol" in value) ||
			value.protocol !== "oqt-guard-input-v1" ||
			!("session" in value) ||
			value.session !== binding.session ||
			!("workspace" in value) ||
			value.workspace !== binding.workspace ||
			!("input" in value) ||
			!value.input ||
			typeof value.input !== "object"
		)
			throw new Error("Guard binding/protocol mismatch");
		return value.input;
	}
	try {
		await bounded(
			async () => {
				await admitted("attach");
				// deadline=0 disables WireClient's startup retry. Late arrivals close.
				const connectedGuard = await WireClient.connect(binding.guardSocket, 0);
				if (disposed) {
					connectedGuard.socket.destroy();
					current();
				}
				guard = connectedGuard;
				void guard.closed.then(cancel);
				await describe();
				const connected = await Client.connect({
					serverId: binding.serverId,
					transportFactory: createUnixTransportFactory({
						path: binding.socket,
					}),
					maxFrameLength: 1024 * 1024,
				});
				if (disposed) {
					await connected.dispose();
					current();
				}
				client = connected;
				client.onConnectionStateChange(({ state }) => {
					if (state !== "connected") cancel();
				});
				client.onAttachmentChange((target) => {
					if (
						(target !== undefined && target.sessionId !== binding.session) ||
						(attachmentId !== undefined &&
							target?.attachmentId !== attachmentId)
					)
						cancel();
				});
				const server = createServerServiceSource(client, { onError: cancel });
				sources.push(server);
				const services = server.open({
					services: [SessionManagement],
					assertAccess: current,
					onError: cancel,
				});
				await services.ready(context);
				current();
				await services.use(SessionManagement).attach(binding.session, context);
				current();
				if (client.attachment?.sessionId !== binding.session)
					throw new Error("Exact session attach failed");
				attachmentId = client.attachment.attachmentId;
				await describe();
			},
			() => false,
		);
	} catch (error) {
		await dispose();
		throw error;
	}
	if (!client) {
		await dispose();
		throw new NativeClientError("Native client unavailable", "not-dispatched");
	}
	const native = client;
	const session = createSessionServiceSource(native, { onError: cancel });
	sources.push(session);
	const agents = session.open({
		services: [AgentController],
		assertAccess: current,
		onError: cancel,
	});
	async function mutate(
		action: Exclude<NativeAction, "attach" | "observe">,
		message?: string,
	): Promise<unknown> {
		current();
		if (busy)
			throw new NativeClientError(
				"Concurrent native mutation refused",
				"not-dispatched",
			);
		if (
			(action === "prompt" ||
				action === "steer" ||
				action === "request-input") &&
			(typeof message !== "string" ||
				!message.length ||
				Buffer.byteLength(message) >
					(action === "request-input" ? 160 : 32_768))
		) {
			throw new NativeClientError(
				"Invalid native action size",
				"not-dispatched",
			);
		}
		busy = true;
		let sent = false;
		try {
			return await bounded(
				async () => {
					await admitted(action);
					const input = await describe();
					if (action === "request-input") {
						if (lease) throw new Error("Input already leased");
						sent = true;
						const result = await guardClient().call(
							"input_request",
							{
								session: binding.session,
								attachment: attachmentId,
								label: message,
							},
							life.signal,
						);
						current();
						if (
							!result ||
							typeof result !== "object" ||
							!("granted" in result) ||
							result.granted !== true
						)
							return result;
						const confirmed = await describe();
						if (!("holder" in confirmed) || confirmed.holder !== attachmentId)
							throw new Error("Input lease binding changed");
						lease = true;
						return result;
					}
					if (action === "release") {
						sent = true;
						await guardClient().call(
							"input_release",
							{ session: binding.session },
							life.signal,
						);
						lease = false;
						return;
					}
					if (!lease || !("holder" in input) || input.holder !== attachmentId)
						throw new Error("Terminal-confirmed input lease required");
					await agents.ready(context);
					current();
					const agent = agents.use(AgentController);
					sent = true;
					return action === "abort"
						? agent.abort(context)
						: agent[action]({ message: message ?? "", images: null }, context);
				},
				() => sent,
			);
		} finally {
			busy = false;
		}
	}
	return Object.freeze({
		requestInput: (label: string) => mutate("request-input", label),
		prompt: (message: string) => mutate("prompt", message),
		steer: (message: string) => mutate("steer", message),
		abort: () => mutate("abort"),
		async release() {
			try {
				await mutate("release");
			} finally {
				await dispose();
			}
		},
		dispose,
		async observe(
			listener: (event: {
				kind: "snapshot" | "update";
				value: unknown;
			}) => void,
		) {
			current();
			if (observing || typeof listener !== "function")
				throw new NativeClientError(
					"Only one raw observer allowed",
					"not-dispatched",
				);
			observing = true;
			await bounded(
				async () => {
					await admitted("observe");
					await describe();
					const target = native.attachment;
					if (!target) throw new Error("Native attachment unavailable");
					const subscription = await native.subscribeService(
						target,
						"pi.transcript",
						"singleton",
						(value) => {
							current();
							try {
								listener({ kind: "update", value });
							} catch {
								cancel();
							}
						},
						life.signal,
					);
					current();
					listener({ kind: "snapshot", value: subscription.snapshot });
					subscription.start();
				},
				() => false,
			);
		},
	});
}
