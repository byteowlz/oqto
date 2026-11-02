/** oqt host contracts on chord2 (ADR-0071). Shared by server, Session workers
 * and clients. Principals are stamped by links; callers never supply them.
 *
 * Two layers. Worker types (`session:<id>`) own a Session's Durable state and
 * accept only the server. Server types (`server`) are the client-facing facade:
 * the guard admits input (remote input leases) and attributes it to the calling
 * client's attachment before the server forwards to the worker. A client can
 * therefore never bypass the guard by addressing a worker directly.
 */
import { hostFamily, hostNamed, method, remote } from "@earendil-works/chord";
import { Type } from "typebox";

export type Principal =
	/** A local client authenticated by the server's private socket; `client` is minted per connection. */
	| { readonly kind: "owner"; readonly client: string }
	/** The server's own requests, as a worker sees them. */
	| { readonly kind: "server" }
	/** A worker calling back to the server. */
	| {
			readonly kind: "worker";
			readonly session: string;
			readonly generation: number;
	  }
	/** A host's own code. */
	| { readonly kind: "host" };

export const principalKey = (principal: Principal): string => {
	switch (principal.kind) {
		case "owner":
			return `owner:${principal.client}`;
		case "worker":
			return `worker:${principal.session}:${principal.generation}`;
		default:
			return principal.kind;
	}
};

/** Opaque Session ids become chord host names (`session:<id>`); keep them simple. */
export const SESSION_ID = /^[A-Za-z0-9_-]{1,64}$/;
const SessionId = Type.String({ pattern: SESSION_ID.source });
const ConversationId = Type.Integer({ minimum: 1 });
const Text = Type.String({ maxLength: 1024 * 1024 });
/** Opaque client request id for exactly-once submission (Durable requestId). */
const RequestId = Type.String({ pattern: "^[A-Za-z0-9_.:-]{1,128}$" });
const ThinkingLevel = Type.Union([
	Type.Literal("off"),
	Type.Literal("minimal"),
	Type.Literal("low"),
	Type.Literal("medium"),
	Type.Literal("high"),
	Type.Literal("xhigh"),
	Type.Literal("max"),
]);
const ModelRef = Type.Object({
	provider: Type.String({ maxLength: 256 }),
	modelId: Type.String({ maxLength: 256 }),
});

/** Accepted or rejected with a message the user may see; never raw internals. */
export const Operation = Type.Union([
	Type.Object({ accepted: Type.Literal(true), id: Type.String() }),
	Type.Object({
		accepted: Type.Literal(false),
		error: Type.String({ maxLength: 2048 }),
	}),
]);

const Input = Type.Object({ text: Text, requestId: Type.Optional(RequestId) });

export const ModelsState = Type.Object({
	revision: Type.Integer(),
	available: Type.Array(
		Type.Object({
			provider: Type.String(),
			modelId: Type.String(),
			name: Type.String(),
			reasoning: Type.Boolean(),
		}),
	),
	refresh: Type.Union([
		Type.Object({
			status: Type.Union([
				Type.Literal("idle"),
				Type.Literal("refreshing"),
				Type.Literal("done"),
			]),
		}),
		Type.Object({
			status: Type.Literal("warning"),
			errors: Type.Record(Type.String(), Type.String()),
		}),
	]),
});

export const ConversationSummary = Type.Object({
	id: ConversationId,
	parent: Type.Optional(
		Type.Object({ conversation: ConversationId, at: Type.String() }),
	),
	entries: Type.Integer(),
	title: Type.Optional(Type.String({ maxLength: 512 })),
});

const ConversationMethods = {
	prompt: method(Input, Operation),
	steer: method(Input, Operation),
	followUp: method(Input, Operation),
	/** Cancels one queued input by its id; reports whether it was still queued. */
	cancelQueued: method(
		Type.Object({ id: Type.String({ maxLength: 128 }) }),
		Type.Object({
			outcome: Type.Union([
				Type.Literal("cancelled"),
				Type.Literal("already_consumed"),
				Type.Literal("not_found"),
			]),
		}),
	),
	abort: method(),
	compact: method(
		Type.Object({ instructions: Type.Optional(Text) }),
		Operation,
	),
	selectModel: method(ModelRef),
	selectThinking: method(Type.Object({ level: ThinkingLevel })),
	cycleThinking: method(),
	thinkingLevels: method(undefined, Type.Array(ThinkingLevel)),
} as const;

// ─── Worker layer (`session:<id>`): server principal only ────────────────────

/** One Session as its worker sees it. */
export const WorkerSession = remote("oqt.worker.session", {
	version: 1,
	input: Type.Object({ session: SessionId }),
	host: hostFamily("session:", "session"),
	variants: {
		default: {
			state: {
				root: ConversationId,
				/** The conversation this Session currently presents (durable selection). */
				selected: ConversationId,
				conversations: Type.Array(ConversationSummary),
				models: ModelsState,
			},
			methods: {
				/** Selects a conversation; fenced by the guard against in-flight operator effects. */
				select: method(
					Type.Object({ conversation: ConversationId }),
					Operation,
				),
				/** Forks `conversation` at entry `at` and selects the fork. */
				fork: method(
					Type.Object({
						conversation: ConversationId,
						at: Type.String({ maxLength: 128 }),
					}),
					Operation,
				),
				refreshModels: method(),
			},
		},
	},
});

/** One conversation as its worker sees it: the Durable view and its operations. */
export const WorkerConversation = remote("oqt.worker.conversation", {
	version: 1,
	input: Type.Object({ session: SessionId, conversation: ConversationId }),
	host: hostFamily("session:", "session"),
	variants: {
		default: {
			state: { view: Type.Unknown() },
			methods: ConversationMethods,
		},
	},
});

// ─── Server layer (`server`): the client-facing facade ───────────────────────

export const SessionSummaryState = Type.Object({
	id: SessionId,
	updatedAt: Type.Number(),
	title: Type.Optional(Type.String({ maxLength: 512 })),
});

/** The workspace's Sessions. */
export const Sessions = remote("oqt.sessions", {
	version: 1,
	input: Type.Object({}),
	host: hostNamed("server"),
	variants: {
		default: {
			state: { sessions: Type.Array(SessionSummaryState) },
			methods: {
				create: method(undefined, Type.Object({ session: SessionId })),
			},
		},
	},
});

/** A Session for one client: selection, conversations, models and input control. */
export const Session = remote("oqt.session", {
	version: 2,
	input: Type.Object({ session: SessionId }),
	host: hostNamed("server"),
	variants: {
		default: {
			state: {
				root: ConversationId,
				selected: ConversationId,
				conversations: Type.Array(ConversationSummary),
				models: ModelsState,
				/** Remote input control as the guard sees it; `holder` is opaque. */
				input: Type.Object({
					available: Type.Boolean(),
					pending: Type.Boolean(),
					holder: Type.Union([Type.String(), Type.Null()]),
					mine: Type.Boolean(),
				}),
			},
			methods: {
				select: method(
					Type.Object({ conversation: ConversationId }),
					Operation,
				),
				fork: method(
					Type.Object({
						conversation: ConversationId,
						at: Type.String({ maxLength: 128 }),
					}),
					Operation,
				),
				refreshModels: method(),
			},
		},
	},
});

/** One conversation for one client. Participants submit; input is admitted by the guard first. */
export const Conversation = remote("oqt.conversation", {
	version: 2,
	input: Type.Object({ session: SessionId, conversation: ConversationId }),
	host: hostNamed("server"),
	variants: {
		viewer: { state: { view: Type.Unknown() } },
		participant: {
			state: { view: Type.Unknown() },
			methods: ConversationMethods,
		},
	},
});
