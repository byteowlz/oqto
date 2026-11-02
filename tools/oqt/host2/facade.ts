/** Server-side facade: the only objects clients can reach (ADR-0071). Each
 * forwards to the Session's worker as the server itself, after the input gate
 * (the guard's remote input leases) admitted the calling client's attachment.
 * Worker objects accept only the server principal, so nothing reaches Durable
 * state without passing here.
 */
import { randomBytes } from "node:crypto";
import { readdir, stat } from "node:fs/promises";
import { join } from "node:path";
import { forbidden, type Host } from "@earendil-works/chord";
import { Type } from "typebox";
import {
	Conversation,
	type Principal,
	SESSION_ID,
	Session,
	SessionSummaryState,
	Sessions,
	WorkerConversation,
	WorkerSession,
} from "./contract.ts";

/** One client's input admission for one mutating call. */
export interface InputAdmission {
	/** Completes the call's accounting (queued ids for cancellation, lease use). */
	done(result: unknown): Promise<void>;
}

/** The guard's remote input leases, keyed by the calling client's attachment. */
export interface InputGate {
	attach(session: string, client: string): void;
	detach(session: string, client: string): void;
	/** Rejects when another client holds input control or the lease is not granted. */
	admit(
		session: string,
		client: string,
		member: string,
	): Promise<InputAdmission>;
	view(
		session: string,
		client: string,
	): {
		available: boolean;
		pending: boolean;
		holder: string | null;
		mine: boolean;
	};
	/** Called with every change of any Session's input view. */
	onChange(listener: () => void): () => void;
}

const MUTATIONS = [
	"prompt",
	"steer",
	"followUp",
	"cancelQueued",
	"abort",
	"compact",
] as const;
const CONFIGURE = ["selectModel", "selectThinking", "cycleThinking"] as const;
const clientOf = (principal: Principal) =>
	principal.kind === "owner"
		? principal.client
		: principal.kind === "host"
			? "host"
			: undefined;

/** A fresh opaque Session id (matches SESSION_ID). */
export const newSessionId = () =>
	randomBytes(12)
		.toString("base64url")
		.replace(/[^A-Za-z0-9_-]/g, "");

export function serveFacade(
	host: Host<Principal>,
	options: { readonly sessionsDirectory: string; readonly input?: InputGate },
) {
	const { input } = options;
	const list = async () => {
		const sessions: { id: string; updatedAt: number }[] = [];
		for (const name of await readdir(options.sessionsDirectory).catch(
			() => [] as string[],
		)) {
			const id = name.endsWith(".sqlite")
				? name.slice(0, -".sqlite".length)
				: "";
			if (!SESSION_ID.test(id)) continue;
			const meta = await stat(join(options.sessionsDirectory, name)).catch(
				() => undefined,
			);
			if (meta?.isFile()) sessions.push({ id, updatedAt: meta.mtimeMs });
		}
		return sessions.sort((a, b) => b.updatedAt - a.updatedAt);
	};
	host.serve(Sessions, (_input, principal, self) => {
		if (!clientOf(principal)) throw forbidden();
		const sessions = self.state("sessions", Type.Array(SessionSummaryState), {
			init: () => list(),
		});
		return {
			variant: "default",
			sessions,
			create: async (ctx) => {
				const session = newSessionId();
				// The Session exists once its worker opens storage; list it eagerly.
				sessions.change(ctx, (draft) => {
					draft.unshift({ id: session, updatedAt: Date.now() });
				});
				return { session };
			},
		};
	});
	host.serve(Session, ({ session }, principal, self) => {
		const client = clientOf(principal);
		if (!client) throw forbidden();
		const worker = self.get(WorkerSession, { session });
		input?.attach(session, client);
		self.resource("attachment", {
			version: 1,
			open: () => client,
			close: () => input?.detach(session, client),
		});
		const inputView = self.state(
			"input",
			Type.Object({
				available: Type.Boolean(),
				pending: Type.Boolean(),
				holder: Type.Union([Type.String(), Type.Null()]),
				mine: Type.Boolean(),
			}),
			input?.view(session, client) ?? {
				available: true,
				pending: false,
				holder: null,
				mine: true,
			},
		);
		if (input)
			self.on(
				"input",
				{
					subscribe: (listener: (value: null) => void) =>
						input.onChange(() => listener(null)),
				},
				(_value, ctx) => inputView.replace(ctx, input.view(session, client)),
			);
		const navigate = async <T>(run: () => Promise<T>) => {
			// Navigation changes where input goes, so it needs input control too.
			const admission = await input?.admit(session, client, "select");
			const result = await run();
			await admission?.done(result);
			return result;
		};
		return {
			variant: "default",
			root: worker.root,
			selected: worker.selected,
			conversations: worker.conversations,
			models: worker.models,
			input: inputView,
			select: (args, ctx) => navigate(() => worker.select(args, ctx)),
			fork: (args, ctx) => navigate(() => worker.fork(args, ctx)),
			refreshModels: (ctx) => worker.refreshModels(ctx),
		};
	});
	host.serve(Conversation, ({ session, conversation }, principal, self) => {
		const client = clientOf(principal);
		if (!client) throw forbidden();
		const worker = self.get(WorkerConversation, { session, conversation });
		const gated =
			<A extends unknown[], R>(
				member: string,
				call: (...args: A) => Promise<R>,
			) =>
			async (...args: A): Promise<R> => {
				const admission = await input?.admit(session, client, member);
				const result = await call(...args);
				await admission?.done(result);
				return result;
			};
		const forward = Object.fromEntries(
			[...MUTATIONS, ...CONFIGURE].map((member) => [
				member,
				gated(member, (...args: unknown[]) =>
					(
						worker as unknown as Record<
							string,
							(...a: unknown[]) => Promise<unknown>
						>
					)[member](...args),
				),
			]),
		) as Record<(typeof MUTATIONS)[number] | (typeof CONFIGURE)[number], never>;
		return {
			variant: "participant",
			view: worker.view,
			...forward,
			thinkingLevels: (ctx) => worker.thinkingLevels(ctx),
		};
	});
}
