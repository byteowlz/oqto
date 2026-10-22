/** Presentation-owned startup input. No transport, storage or authority lives
 * here. A transport adapter must validate readiness and bind exact identities
 * before calling ready(). A queued item is never retried after dispatch starts.
 */
export interface PresentationBinding {
	readonly runtime: string;
	readonly session: string;
	readonly conversation: string;
	readonly model: string;
	readonly guardEpoch: string;
}
export interface StartupSubmission {
	readonly requestId: string;
	readonly text: string;
	readonly binding: PresentationBinding;
}
export interface PresentationStartupView {
	readonly phase: "loading" | "ready" | "failed" | "closed";
	readonly draft: string;
	readonly queued: boolean;
	readonly sending: boolean;
	/** Unsent/uncertain startup text kept separately from a newer editor draft. */
	readonly retained: readonly {
		readonly requestId: string;
		readonly text: string;
	}[];
	readonly status:
		| "loading"
		| "queued"
		| "ready"
		| "sending"
		| "failed"
		| "uncertain"
		| "closed";
}
interface Pending {
	readonly requestId: string;
	readonly text: string;
}
const fields = [
	"runtime",
	"session",
	"conversation",
	"model",
	"guardEpoch",
] as const;
function copyBinding(binding: PresentationBinding): PresentationBinding {
	for (const field of fields) {
		const value = binding[field];
		if (typeof value !== "string" || !value.length || value.length > 1024)
			throw new Error("Incomplete presentation binding");
	}
	return Object.freeze({
		runtime: binding.runtime,
		session: binding.session,
		conversation: binding.conversation,
		model: binding.model,
		guardEpoch: binding.guardEpoch,
	});
}

/** Supervisor retains the returned ready/fail/close controls. The editor gets
 * only input. dispatch must resolve only after authoritative admission/denial;
 * throwing means uncertain admission and is never an instruction to retry.
 * No subscription notification contains prompt text: consumers read view().
 */
export function createPresentationStartup(options: {
	readonly launchId: string;
	readonly runtime: string;
	readonly expectedSession?: string;
	readonly dispatch: (
		submission: StartupSubmission,
	) => Promise<"accepted" | "rejected">;
	readonly changed: () => void;
}) {
	if (!options.launchId || !options.runtime)
		throw new Error("Startup identity required");
	const runtime = options.runtime;
	const expectedSession = options.expectedSession;
	const launchId = options.launchId;
	let sequence = 0;
	let phase: PresentationStartupView["phase"] = "loading";
	let status: PresentationStartupView["status"] = "loading";
	let draft = "";
	let pending: Pending | undefined;
	let sending: Pending | undefined;
	let binding: PresentationBinding | undefined;
	const retained = new Map<string, Pending>();
	let epoch = 0;
	const changed = () => {
		// A renderer failure must not change admission state or trigger a retry.
		try {
			options.changed();
		} catch {
			/* presentation only */
		}
	};
	async function drain() {
		if (phase !== "ready" || !binding || !pending || sending) return;
		const item = pending;
		const target = binding;
		const dispatchEpoch = epoch;
		pending = undefined;
		sending = item;
		status = "sending";
		changed();
		// A renderer may synchronously close/fail while handling notification.
		if (dispatchEpoch !== epoch || phase !== "ready") {
			sending = undefined;
			return;
		}
		try {
			const result = await options.dispatch(
				Object.freeze({ ...item, binding: target }),
			);
			if (dispatchEpoch !== epoch || phase !== "ready") return;
			if (result !== "accepted" && result !== "rejected")
				throw new Error("Invalid admission response");
			status = result === "accepted" ? "ready" : "failed";
			if (result === "rejected") {
				retained.set(item.requestId, item);
				if (!draft) draft = item.text;
			}
		} catch {
			if (dispatchEpoch === epoch && phase === "ready") {
				status = "uncertain";
				retained.set(item.requestId, item);
				// Keep text for recovery, but don't turn it into a queued retry.
				if (!draft) draft = item.text;
			}
		} finally {
			sending = undefined;
			changed();
		}
	}
	const input = Object.freeze({
		view(): PresentationStartupView {
			return Object.freeze({
				phase,
				draft,
				queued: pending !== undefined,
				sending: sending !== undefined,
				status,
				retained: Object.freeze([...retained.values()]),
			});
		},
		edit(text: string) {
			if (phase === "closed") return;
			if (text.length > 1024 * 1024)
				throw new Error("Startup draft exceeds limit");
			draft = text;
			changed();
		},
		submit() {
			if (
				phase === "failed" ||
				phase === "closed" ||
				pending ||
				sending ||
				status === "uncertain" ||
				retained.size >= 64
			)
				return false;
			if (!draft.trim()) return false;
			pending = Object.freeze({
				requestId: `${launchId}:${++sequence}`,
				text: draft,
			});
			draft = "";
			status = "queued";
			changed();
			void drain();
			return true;
		},
		cancel() {
			if (!pending) return false;
			// Never overwrite a newer draft; the cancelled text remains observable.
			const text = pending.text;
			retained.set(pending.requestId, pending);
			pending = undefined;
			if (!draft) draft = text;
			status = phase === "ready" ? "ready" : "loading";
			changed();
			return text;
		},
	});
	return Object.freeze({
		input,
		ready(candidate: PresentationBinding) {
			if (phase !== "loading")
				throw new Error("Startup is not awaiting readiness");
			const target = copyBinding(candidate);
			if (
				target.runtime !== runtime ||
				(expectedSession !== undefined && target.session !== expectedSession)
			)
				throw new Error("Startup target changed");
			binding = target;
			phase = "ready";
			status = pending ? "queued" : "ready";
			changed();
			return drain();
		},
		fail() {
			if (phase === "closed") return;
			epoch++;
			phase = "failed";
			status = sending ? "uncertain" : "failed";
			const interrupted = pending ?? sending;
			if (interrupted) retained.set(interrupted.requestId, interrupted);
			pending = undefined;
			if (!draft && interrupted) draft = interrupted.text;
			changed();
			return interrupted?.text;
		},
		close() {
			epoch++;
			phase = "closed";
			status = "closed";
			pending = undefined;
			binding = undefined;
			changed();
		},
	});
}
