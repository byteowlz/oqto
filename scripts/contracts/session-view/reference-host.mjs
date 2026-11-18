// Reference mapping of the session-view contract (v0) onto upstream Pi Durable.
// In-process only: no socket, no authentication, no sandbox. It exists to capture golden
// frames from the real harness and to show the mapping a host has to implement.
// It is not a host to deploy.
import { createHash } from "node:crypto";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import { ConversationBusy, ConversationIdle } from "@earendil-works/pi-durable";

export const CONTRACT = "oqto.session-view";
export const VERSION = 0;
export const LIMITS = {
	max_request_bytes: 262144,
	max_text_bytes: 32768,
	max_write_data_bytes: 65536,
	max_id_bytes: 128,
	max_frame_bytes: 8388608,
	read_limit: { default: 50, minimum: 1, maximum: 200 },
	max_cursor_bytes: 4096,
	window_min: { default: 50, minimum: 1, maximum: 200 },
	window_max: { maximum: 400 },
};
const ID = /^[A-Za-z0-9._:-]{1,128}$/;
const INPUT_KINDS = ["prompt", "steer", "follow_up"];
const ctx = BACKGROUND_CONTEXT;
const plain = (value) => JSON.parse(JSON.stringify(value));
/** Canonical JSON: sorted keys, no whitespace. Defines "same content" for request ids. */
const canonical = (value) => {
	if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
	if (value !== null && typeof value === "object") {
		return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
	}
	return JSON.stringify(value);
};
const digest = (value) => createHash("sha256").update(canonical(value)).digest("hex");
const encodeCursor = (cursor) => Buffer.from(JSON.stringify(cursor)).toString("base64url");
const decodeCursor = (text) => {
	if (typeof text !== "string" || text === "" || Buffer.byteLength(text) > LIMITS.max_cursor_bytes || !/^[A-Za-z0-9_-]+$/.test(text)) return undefined;
	try {
		const value = JSON.parse(Buffer.from(text, "base64url").toString("utf8"));
		return value !== null && typeof value === "object" && !Array.isArray(value) ? value : undefined;
	} catch {
		return undefined;
	}
};
const bytes = (value) => Buffer.byteLength(typeof value === "string" ? value : JSON.stringify(value));

/**
 * @param {{ harness: object, sessions: Record<string, object>, store: string, upstream: { package: string, version: string }, emit: (frame: object) => void }} options
 * `sessions` maps public Session ids to upstream conversations. `store` names the storage the
 * Session's entries live in; it changes whenever that storage is replaced, so clients know when a
 * cache keyed by entry id is no longer valid. `emit` receives every host-to-client frame (replies
 * and pushed view, receipt and ended frames) for the first connection, in order; `connect(emit)`
 * opens another connection. Receipt changes go to the requesting connection and to every
 * connection attached to the Session.
 */
/** Revision 2.2: the passive entry kind of a generative UI part (ADR-0074). */
export const UI_KIND = "oqto.ui";

export function createReferenceHost({ harness, sessions, store, upstream, emit }) {
	let attachmentCounter = 0;
	let closing = false;
	const attachments = new Map(); // attachment id -> { session, seq, detach, conn }
	const connections = new Set(); // { emit, ids }
	const requests = new Map(); // `${session}\n${request_id}` -> { digest, kind, receipt, submission }
	const error = (id, code, message) => ({ type: "error", id, code, message });

	const receiptOf = (session, requestId, kind, record) => {
		const receipt = { type: "receipt", session, request_id: requestId, kind, status: record.status };
		for (const field of ["entry", "answer", "reason", "detail"]) {
			if (record[field] !== undefined) receipt[field] = plain(record[field]);
		}
		return receipt;
	};

	/** Push a receipt when a tracked submission's durable status changed. */
	const refresh = async (key) => {
		const tracked = requests.get(key);
		if (tracked === undefined || tracked.submission === undefined) return;
		const [session, requestId] = key.split("\n");
		const next = receiptOf(session, requestId, tracked.kind, await tracked.submission.status(ctx));
		if (JSON.stringify(next) === JSON.stringify(tracked.receipt)) return;
		tracked.receipt = next;
		deliver(next, tracked.conn);
		if (next.status === "done" || next.status === "unanswered") tracked.submission = undefined;
	};
	/** A receipt goes to its requester, attached or not, and to every connection attached to its Session. */
	const deliver = (receipt, requester, except) => {
		const targets = new Set(requester === undefined ? [] : [requester]);
		for (const state of attachments.values()) if (state.session === receipt.session) targets.add(state.conn);
		for (const conn of connections) if (targets.has(conn) && conn !== except) conn.emit(receipt);
	};
	const refreshAll = async () => {
		for (const key of [...requests.keys()]) await refresh(key);
	};

	const admit = (frame, kind, payload) => {
		const key = `${frame.session}\n${frame.request_id}`;
		const fingerprint = digest([kind, payload]);
		const known = requests.get(key);
		if (known !== undefined && known.digest !== fingerprint) return { conflict: true };
		return { key, fingerprint, known };
	};

	const handlers = {
		async attach(frame, conn) {
			if (frame.v !== VERSION) return { ...error(frame.id, "unsupported_version", "supported: 0"), supported: [VERSION] };
			const conversation = sessions[frame.session];
			if (conversation === undefined) return error(frame.id, "unknown_session", "no such Session");
			const window = frame.window ?? {};
			const min = window.min ?? LIMITS.window_min.default;
			const max = window.max ?? 2 * min;
			if (
				!Number.isInteger(min) || min < LIMITS.window_min.minimum || min > LIMITS.window_min.maximum ||
				!Number.isInteger(max) || max < min || max > LIMITS.window_max.maximum ||
				(window.from !== undefined && (!Number.isInteger(window.from) || window.from < 0))
			) return error(frame.id, "invalid", "window out of bounds");
			const attachment = `att_${++attachmentCounter}`;
			const state = { session: frame.session, seq: 0, detach: () => {}, pending: [], conn };
			const view = (ops, retired) => {
				const out = { type: "view", attachment, seq: ++state.seq, ops: plain(ops) };
				if (retired) out.retired = true;
				conn.emit(out);
			};
			const observer = {
				advance(call) {
					if (call.kind !== "value") return;
					// Changes that arrive before the base frame went out wait behind it, in order.
					if (state.pending !== undefined) state.pending.push([call.ops, call.retired]);
					else view(call.ops, call.retired);
					// Receipts follow the commit that changed them; never derived from streamed text.
					queueMicrotask(() => void refreshAll());
				},
				end(end) {
					if (!attachments.delete(attachment)) return;
					conn.emit({ type: "ended", attachment, reason: closing ? "host_closed" : "failed" });
				},
			};
			const upstreamWindow = { min, max, ...(window.from === undefined ? {} : { from: window.from }) };
			const observation = await harness.observeConversation(conversation.id, observer, ctx, upstreamWindow);
			state.detach = observation.detach;
			attachments.set(attachment, state);
			return {
				reply: {
					type: "attached", id: frame.id, v: VERSION, attachment, session: frame.session, store,
					window: { min, max }, upstream, max_frame_bytes: LIMITS.max_frame_bytes,
				},
				// The reply goes out first, then the base frame, then anything that queued meanwhile.
				after() {
					view([["r", observation.value]], false);
					const queued = state.pending;
					state.pending = undefined;
					for (const [ops, retired] of queued) view(ops, retired);
				},
			};
		},

		async detach(frame, conn) {
			const state = attachments.get(frame.attachment);
			if (state === undefined || state.conn !== conn) return error(frame.id, "unknown_attachment", "no such attachment");
			attachments.delete(frame.attachment);
			state.detach();
			return { type: "detached", id: frame.id, attachment: frame.attachment };
		},

		async submit(frame, conn) {
			const conversation = sessions[frame.session];
			if (conversation === undefined) return error(frame.id, "unknown_session", "no such Session");
			const input = frame.input ?? {};
			if (!INPUT_KINDS.includes(input.kind) || typeof input.text !== "string" || input.text.trim() === "") {
				return error(frame.id, "invalid", "input needs kind and non-empty text");
			}
			if (bytes(input.text) > LIMITS.max_text_bytes) return error(frame.id, "too_large", "text exceeds limit");
			const admission = admit(frame, input.kind, input.text);
			if (admission.conflict) return error(frame.id, "request_conflict", "request_id was used with different content");
			if (admission.known !== undefined) return { ...admission.known.receipt, id: frame.id };
			const draft = { type: "input", content: input.text, requestId: frame.request_id };
			if (input.kind === "prompt") draft.whenBusy = "reject";
			if (input.kind === "steer") Object.assign(draft, { whenBusy: "steer", whenIdle: "reject" });
			let submission;
			try {
				submission = await conversation.submit(draft, ctx);
			} catch (cause) {
				if (cause instanceof ConversationBusy) return error(frame.id, "busy", "a run is working; use steer or follow_up");
				if (cause instanceof ConversationIdle) return error(frame.id, "idle", "no run is working; use prompt or follow_up");
				throw cause;
			}
			const receipt = receiptOf(frame.session, frame.request_id, input.kind, await submission.status(ctx));
			requests.set(admission.key, { digest: admission.fingerprint, kind: input.kind, receipt, submission, conn });
			return { reply: { ...receipt, id: frame.id }, fanout: true };
		},

		async write(frame, conn) {
			const conversation = sessions[frame.session];
			if (conversation === undefined) return error(frame.id, "unknown_session", "no such Session");
			const entry = frame.entry ?? {};
			if (typeof entry.kind !== "string" || !/^[a-z][a-z0-9_.-]{0,63}$/.test(entry.kind) || entry.kind.startsWith("pi.")) {
				return error(frame.id, "invalid", "entry kind must be lowercase and must not start with pi.");
			}
			// Revision 2.2: generative UI parts are written by the host only, never by a client.
			if (entry.kind === UI_KIND) return error(frame.id, "invalid", "oqto.ui entries are written by the host");
			if (bytes(entry.data ?? null) > LIMITS.max_write_data_bytes) return error(frame.id, "too_large", "entry data exceeds limit");
			const admission = admit(frame, "write", entry);
			if (admission.conflict) return error(frame.id, "request_conflict", "request_id was used with different content");
			if (admission.known !== undefined) return { ...admission.known.receipt, id: frame.id };
			// Only kind and data: such a write changes nothing the model sees and never starts a run.
			const draft = { type: "write", requestId: frame.request_id, entry: { kind: entry.kind, ...(entry.data === undefined ? {} : { data: entry.data }) } };
			const submission = await conversation.submit(draft, ctx);
			const record = await submission.wait(ctx);
			const receipt = receiptOf(frame.session, frame.request_id, "write", record);
			requests.set(admission.key, { digest: admission.fingerprint, kind: "write", receipt, conn });
			return { reply: { ...receipt, id: frame.id }, fanout: true };
		},

		async abort(frame, conn) {
			const conversation = sessions[frame.session];
			if (conversation === undefined) return error(frame.id, "unknown_session", "no such Session");
			const admission = admit(frame, "abort", null);
			if (admission.conflict) return error(frame.id, "request_conflict", "request_id was used with different content");
			if (admission.known !== undefined) return { ...admission.known.receipt, id: frame.id };
			// Resolves once the conversation is idle. Stopping work is not rollback.
			await conversation.abort(ctx);
			const receipt = { type: "receipt", session: frame.session, request_id: frame.request_id, kind: "abort", status: "done" };
			requests.set(admission.key, { digest: admission.fingerprint, kind: "abort", receipt, conn });
			await refreshAll();
			return { reply: { ...receipt, id: frame.id }, fanout: true };
		},

		async read_entries(frame) {
			const conversation = sessions[frame.session];
			if (conversation === undefined) return error(frame.id, "unknown_session", "no such Session");
			const order = frame.order ?? "descending";
			const limit = frame.limit ?? LIMITS.read_limit.default;
			const bound = (value) => value === undefined || (Number.isInteger(value) && value >= 0);
			if (
				!["ascending", "descending"].includes(order) || !Number.isInteger(limit) ||
				limit < LIMITS.read_limit.minimum || limit > LIMITS.read_limit.maximum ||
				!bound(frame.min) || !bound(frame.max) || (frame.min !== undefined && frame.max !== undefined && frame.min > frame.max)
			) return error(frame.id, "invalid", "read bounds, order or limit out of range");
			let cursor;
			if (frame.cursor !== undefined) {
				cursor = decodeCursor(frame.cursor);
				if (cursor === undefined) return error(frame.id, "invalid", "cursor malformed");
			}
			const query = { order, ...(frame.min === undefined ? {} : { minEntryId: frame.min }), ...(frame.max === undefined ? {} : { maxEntryId: frame.max }) };
			// A page that does not fit one frame is read again with a smaller limit. Entries never
			// change, so a smaller page of the same scan is still exact.
			for (let take = limit; take >= 1; take = Math.floor(take / 2)) {
				let page;
				try {
					page = await conversation.entries(query, take, cursor, ctx);
				} catch {
					return error(frame.id, "invalid", "cursor does not belong to this read");
				}
				const reply = { type: "entries", id: frame.id, session: frame.session, items: plain(page.items) };
				if (page.next !== undefined) reply.next = encodeCursor(page.next);
				if (bytes(reply) <= LIMITS.max_frame_bytes) return reply;
			}
			return error(frame.id, "too_large", "one entry exceeds the frame limit");
		},

		async receipt_query(frame) {
			if (sessions[frame.session] === undefined) return error(frame.id, "unknown_session", "no such Session");
			const key = `${frame.session}\n${frame.request_id}`;
			await refresh(key);
			const known = requests.get(key);
			if (known === undefined) return error(frame.id, "unknown_request", "no such request_id for this Session");
			return { ...known.receipt, id: frame.id };
		},
	};

	const respond = async (frame, conn) => {
		// A request that cannot be answered by a unique id leaves the client unable to match
		// replies: the host closes the connection instead of inventing or repeating an id.
		if (frame === null || typeof frame !== "object" || Array.isArray(frame)) return { close: "not_an_object" };
		if (typeof frame.id !== "string" || !ID.test(frame.id)) return { close: "invalid_id" };
		if (conn.ids.has(frame.id)) return { close: "duplicate_id" };
		conn.ids.add(frame.id);
		const id = frame.id;
		if (bytes(frame) > LIMITS.max_request_bytes) return error(id, "too_large", "request exceeds limit");
		const handler = Object.hasOwn(handlers, frame.type) ? handlers[frame.type] : undefined;
		if (handler === undefined) return error(id, "unsupported", "unknown request type");
		for (const field of ["session", "attachment", "request_id"]) {
			if (frame[field] !== undefined && (typeof frame[field] !== "string" || !ID.test(frame[field]))) {
				return error(id, "invalid", `${field} malformed`);
			}
		}
		if (frame.cursor !== undefined && frame.type !== "read_entries") return error(id, "invalid", "unexpected cursor");
		if (["submit", "write", "abort", "receipt_query"].includes(frame.type) && frame.request_id === undefined) {
			return error(id, "invalid", "request_id required");
		}
		return handler(frame, conn);
	};

	const connect = (connEmit) => {
		const conn = { emit: connEmit, ids: new Set() };
		connections.add(conn);
		return {
			/**
			 * One request in. Its reply goes out through this connection's `emit`, in order with
			 * pushed frames, and is also returned. A new request's first receipt is also pushed to
			 * the other connections attached to its Session. Malformed requests are refused before
			 * any upstream call. A request without a usable, unused id returns `{ close }`: the
			 * binding must close the connection.
			 */
			async request(frame) {
				const result = await respond(frame, conn);
				if (result.close !== undefined) {
					// A closed connection's attachments end silently; it gets nothing more.
					connections.delete(conn);
					for (const [attachment, state] of attachments) {
						if (state.conn !== conn) continue;
						attachments.delete(attachment);
						state.detach();
					}
					return result;
				}
				const reply = result.reply ?? result;
				conn.emit(reply);
				result.after?.();
				if (result.fanout) {
					const { id: _id, ...pushed } = reply;
					deliver(pushed, undefined, conn);
				}
				return reply;
			},
		};
	};
	const first = connect(emit);

	return {
		request: first.request,
		connect,
		/**
		 * Revision 2.2: a host-originated passive write, for example a generative UI part a tool
		 * shows. It is an ordinary entry, so it reaches clients through their views; it is not a
		 * client request and has no receipt. Resolves once the entry is committed.
		 */
		async append(session, requestId, entry) {
			const conversation = sessions[session];
			if (conversation === undefined) throw new Error("no such Session");
			if (bytes(entry.data ?? null) > LIMITS.max_write_data_bytes) throw new Error("entry data exceeds limit");
			const submission = await conversation.submit({ type: "write", requestId, entry: { kind: entry.kind, data: entry.data } }, ctx);
			return submission.wait(ctx);
		},
		/** Call before closing the harness, so attachments end as `host_closed` rather than `failed`. */
		closing() {
			closing = true;
		},
		/** Let pending receipt changes settle (capture helper). */
		settle: refreshAll,
	};
}
