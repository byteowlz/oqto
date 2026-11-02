/** Startup input queue for a presentation whose transport is not ready yet.
 * The editor owns the live draft; this module owns at most one submitted
 * startup message. No transport, storage or authority lives here. The adapter
 * binds exact identities and calls ready(); dispatch happens at most once.
 */
export interface PresentationBinding {
	readonly runtime: string;
	readonly session: string;
	readonly conversation: string;
	readonly model: string;
	/** Attached host process identity (pid and guard operator endpoint). */
	readonly host: string;
}
export interface StartupSubmission {
	readonly requestId: string;
	readonly text: string;
	readonly binding: PresentationBinding;
}
export type StartupStatus =
	| "loading"
	| "idle"
	| "queued"
	| "sending"
	| "sent"
	| "rejected"
	| "uncertain"
	| "failed"
	| "closed";
export interface PresentationStartupView {
	readonly phase: "loading" | "ready" | "failed" | "closed";
	readonly status: StartupStatus;
	readonly queued: boolean;
	readonly sending: boolean;
	/** Text that was submitted at startup but not admitted (rejected, cancelled,
	 * failed, or uncertain). The adapter restores it without replaying it. */
	readonly retained: readonly {
		readonly requestId: string;
		readonly text: string;
	}[];
}
interface Item {
	readonly requestId: string;
	readonly text: string;
}
const fields = ["runtime", "session", "conversation", "model", "host"] as const;
const MAX_TEXT = 1024 * 1024;
const MAX_RETAINED = 64;

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
		host: binding.host,
	});
}

/** dispatch resolves only with an authoritative admission result. Throwing
 * means admission is uncertain: the text is retained and never retried here.
 * changed() carries no text; renderers read view().
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
	const { launchId, runtime, expectedSession } = options;
	let sequence = 0;
	let phase: PresentationStartupView["phase"] = "loading";
	let status: StartupStatus = "loading";
	let pending: Item | undefined;
	let sending: Item | undefined;
	let binding: PresentationBinding | undefined;
	let epoch = 0;
	const retained = new Map<string, Item>();
	const retain = (item: Item) => {
		retained.set(item.requestId, item);
		while (retained.size > MAX_RETAINED)
			retained.delete(retained.keys().next().value as string);
	};
	const changed = () => {
		// A renderer failure must not alter admission state or cause a retry.
		try {
			options.changed();
		} catch {
			/* presentation only */
		}
	};
	const live = (dispatchEpoch: number) =>
		dispatchEpoch === epoch && phase === "ready";
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
		if (!live(dispatchEpoch)) {
			sending = undefined;
			retain(item);
			return;
		}
		try {
			const result = await options.dispatch(
				Object.freeze({ ...item, binding: target }),
			);
			if (result !== "accepted" && result !== "rejected")
				throw new Error("Invalid admission response");
			if (!live(dispatchEpoch)) {
				// Closed/failed during dispatch: the outcome is known only to the
				// transport. Keep the text so it is never silently lost.
				if (result === "rejected") retain(item);
				return;
			}
			status = result === "accepted" ? "sent" : "rejected";
			if (result === "rejected") retain(item);
		} catch {
			retain(item);
			if (live(dispatchEpoch)) status = "uncertain";
		} finally {
			sending = undefined;
			changed();
		}
	}
	const input = Object.freeze({
		view(): PresentationStartupView {
			return Object.freeze({
				phase,
				status,
				queued: pending !== undefined,
				sending: sending !== undefined,
				retained: Object.freeze([...retained.values()]),
			});
		},
		/** Queue one message. False means the caller keeps the text in its editor. */
		submit(text: string): boolean {
			if (phase === "failed" || phase === "closed") return false;
			if (pending || sending || status === "uncertain") return false;
			if (!text.trim() || text.length > MAX_TEXT) return false;
			pending = Object.freeze({ requestId: `${launchId}:${++sequence}`, text });
			status = "queued";
			changed();
			void drain();
			return true;
		},
		/** Withdraw the queued message before dispatch; returns its text. */
		cancel(): string | undefined {
			if (!pending) return undefined;
			const item = pending;
			pending = undefined;
			retain(item);
			status = phase === "ready" ? "idle" : "loading";
			changed();
			return item.text;
		},
		/** Remove a retained item after the adapter restored it to the editor.
		 * Silent: the adapter calls this while rendering, so it must not re-enter. */
		release(requestId: string): void {
			retained.delete(requestId);
		},
	});
	return Object.freeze({
		input,
		ready(candidate: PresentationBinding): Promise<void> {
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
			if (!pending) status = "idle";
			changed();
			return drain();
		},
		/** Startup failed: keep queued text, never dispatch it. */
		fail(): string | undefined {
			if (phase === "closed") return undefined;
			epoch++;
			phase = "failed";
			status = sending ? "uncertain" : "failed";
			const interrupted = pending ?? sending;
			pending = undefined;
			if (interrupted) retain(interrupted);
			changed();
			return interrupted?.text;
		},
		close(): void {
			epoch++;
			phase = "closed";
			status = "closed";
			pending = undefined;
			binding = undefined;
			changed();
		},
	});
}
