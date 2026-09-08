import { randomUUID } from "node:crypto";

const TERMINAL = new Set([
	"saved",
	"saved_refresh_required",
	"failed",
	"cancelled",
	"expired",
]);
const MAX_TEXT = 8192;
function text(value) {
	if (typeof value !== "string" || value.length > MAX_TEXT)
		throw new Error("Invalid authentication interaction");
	return value;
}
function link(value) {
	const url = new URL(text(value));
	if (url.protocol !== "https:" || url.username || url.password)
		throw new Error("Unsupported authentication link");
	return url.href;
}
function eventView(event) {
	switch (event.type) {
		case "auth_url":
			return {
				type: event.type,
				url: link(event.url),
				instructions: text(event.instructions ?? ""),
			};
		case "device_code":
			return {
				type: event.type,
				userCode: text(event.userCode),
				verificationUri: link(event.verificationUri),
			};
		case "progress":
			return { type: event.type, message: text(event.message) };
		case "info":
			return {
				type: event.type,
				message: text(event.message),
				links: (event.links ?? []).slice(0, 8).map((item) => ({
					url: link(item.url),
					label: text(item.label ?? "Open"),
				})),
			};
		default:
			throw new Error("Unsupported authentication event");
	}
}

/** One credential-store owner. Transport supplies the authenticated actor, never request JSON. */
export class ProviderLoginBroker {
	#runtime;
	#machine;
	#authorize;
	#attempt;
	#ttl;
	#requireCommit;
	constructor({
		runtime,
		machine,
		authorize,
		ttlMs = 600_000,
		requireCommit = false,
	}) {
		if (
			!machine ||
			!Number.isSafeInteger(ttlMs) ||
			ttlMs <= 0 ||
			ttlMs > 600_000
		)
			throw new Error("Invalid login policy");
		this.#runtime = runtime;
		this.#machine = machine;
		this.#authorize = authorize;
		this.#ttl = ttlMs;
		this.#requireCommit = requireCommit;
	}
	async #admit(actor, provider) {
		if (!(await this.#authorize(actor, this.#machine, provider)))
			throw new Error("Provider login access denied");
	}
	async providers(actor) {
		await this.#admit(actor);
		return this.#runtime.providers();
	}
	#view(attempt) {
		return structuredClone({
			id: attempt.id,
			machine: this.#machine,
			provider: attempt.provider,
			method: attempt.method,
			state: attempt.state,
			expiresAt: attempt.expiresAt,
			events: attempt.events,
			prompt: attempt.prompt,
			commitNonce: attempt.commitNonce ?? null,
		});
	}
	async #owned(actor, id) {
		const attempt = this.#attempt;
		if (!attempt || attempt.actor !== actor || attempt.id !== id)
			throw new Error("Login attempt unavailable");
		try {
			await this.#admit(actor, attempt.provider);
		} catch (error) {
			this.#stop(attempt, "cancelled");
			throw error;
		}
		return attempt;
	}
	async start(actor, provider, method) {
		await this.#admit(actor, provider);
		if (this.#attempt && !TERMINAL.has(this.#attempt.state))
			throw new Error("A provider login is already running");
		const available = await this.#runtime.providers();
		if (
			!available.some(
				(item) => item.id === provider && item.methods.includes(method),
			)
		)
			throw new Error("Provider login method unavailable");
		// Re-check after the awaited catalog lookup to fence simultaneous starts.
		if (this.#attempt && !TERMINAL.has(this.#attempt.state))
			throw new Error("A provider login is already running");
		const attempt = {
			id: randomUUID(),
			actor,
			provider,
			method,
			state: "running",
			events: [],
			prompt: null,
			controller: new AbortController(),
			expiresAt: Date.now() + this.#ttl,
			pending: null,
			stopReason: null,
		};
		this.#attempt = attempt;
		attempt.timer = setTimeout(() => this.#stop(attempt, "expired"), this.#ttl);
		attempt.timer.unref?.();
		void this.#run(attempt);
		return this.#view(attempt);
	}
	#stop(attempt, reason) {
		if (TERMINAL.has(attempt.state)) return;
		attempt.stopReason = reason;
		attempt.state = "cancelling";
		attempt.events = [];
		attempt.commitNonce = null;
		attempt.controller.abort(new Error("Authentication cancelled"));
		attempt.pending?.reject(new Error("Authentication cancelled"));
	}
	async #run(attempt) {
		try {
			await this.#runtime.login(
				attempt.provider,
				attempt.method,
				{
					signal: attempt.controller.signal,
					notify: (event) => {
						if (attempt.controller.signal.aborted) return;
						attempt.events.push(eventView(event));
						if (attempt.events.length > 16) attempt.events.shift();
					},
					prompt: (prompt) => this.#prompt(attempt, prompt),
				},
				async () => {
					if (this.#requireCommit) {
						attempt.controller.signal.throwIfAborted();
						attempt.state = "awaiting_commit";
						attempt.commitNonce = randomUUID();
						await new Promise((resolve, reject) => {
							attempt.pending = { resolve, reject };
						});
						attempt.pending = null;
						attempt.commitNonce = null;
					}
					await this.#admit(attempt.actor, attempt.provider);
					attempt.controller.signal.throwIfAborted();
				},
			);
			// Pi owns the commit point: a cancellation arriving after commit cannot undo login.
			attempt.state = "saved";
		} catch (error) {
			attempt.state = this.#runtime.credentialsCommitted(error)
				? "saved_refresh_required"
				: (attempt.stopReason ?? "failed");
		} finally {
			clearTimeout(attempt.timer);
			attempt.pending?.reject(new Error("Authentication finished"));
			attempt.pending = null;
			attempt.prompt = null;
			attempt.events = [];
			attempt.commitNonce = null;
		}
	}
	#prompt(attempt, prompt) {
		const signal = prompt.signal
			? AbortSignal.any([attempt.controller.signal, prompt.signal])
			: attempt.controller.signal;
		if (signal.aborted)
			return Promise.reject(new Error("Authentication prompt cancelled"));
		if (attempt.pending)
			return Promise.reject(
				new Error("Concurrent authentication prompts unsupported"),
			);
		if (!["text", "secret", "select", "manual_code"].includes(prompt.type))
			return Promise.reject(new Error("Unsupported authentication prompt"));
		const view = {
			id: randomUUID(),
			type: prompt.type,
			message: text(prompt.message),
			placeholder: text(prompt.placeholder ?? ""),
		};
		if (prompt.type === "select") {
			if (!Array.isArray(prompt.options) || prompt.options.length > 32)
				return Promise.reject(new Error("Invalid authentication choices"));
			view.options = prompt.options.map((item) => ({
				id: text(item.id),
				label: text(item.label),
			}));
		}
		attempt.prompt = view;
		return new Promise((resolve, reject) => {
			const cleanup = () => {
				signal.removeEventListener("abort", abort);
				if (attempt.prompt?.id === view.id) {
					attempt.prompt = null;
					attempt.pending = null;
				}
			};
			const abort = () => {
				cleanup();
				reject(new Error("Authentication prompt cancelled"));
			};
			attempt.pending = {
				resolve: (value) => {
					cleanup();
					resolve(value);
				},
				reject: (error) => {
					cleanup();
					reject(error);
				},
			};
			signal.addEventListener("abort", abort, { once: true });
		});
	}
	async status(actor, id) {
		return this.#view(await this.#owned(actor, id));
	}
	async answer(actor, id, promptId, value) {
		const attempt = await this.#owned(actor, id);
		if (
			attempt.state !== "running" ||
			attempt.prompt?.id !== promptId ||
			!attempt.pending
		)
			throw new Error("Authentication prompt expired");
		text(value);
		if (
			attempt.prompt.type === "select" &&
			!attempt.prompt.options.some((option) => option.id === value)
		)
			throw new Error("Invalid authentication choice");
		attempt.pending.resolve(value); // Deliberately not retained in snapshots or logs.
		return this.#view(attempt);
	}
	async commit(actor, id, nonce) {
		const attempt = await this.#owned(actor, id);
		if (
			attempt.state !== "awaiting_commit" ||
			attempt.commitNonce !== nonce ||
			!attempt.pending
		)
			throw new Error("Credential commit unavailable");
		attempt.state = "running";
		attempt.pending.resolve();
		return this.#view(attempt);
	}
	async cancel(actor, id) {
		const attempt = await this.#owned(actor, id);
		this.#stop(attempt, "cancelled");
		return this.#view(attempt);
	}
	close() {
		if (this.#attempt) this.#stop(this.#attempt, "cancelled");
	}
}
