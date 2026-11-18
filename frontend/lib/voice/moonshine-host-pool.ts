import type { Recognizer } from "@byteowlz/ears-browser";

/**
 * Keeps one Moonshine worker and its loaded model warm across dictations
 * (oqto-623a). Without it every dictation, including each wake-triggered one,
 * spawned a fresh worker, re-initialised WebAssembly and reloaded the model,
 * then terminated the worker again when the dictation ended.
 *
 * Each recognition still gets its own lease: a fresh stream on the shared
 * worker, closed (not the worker) when the recognition ends. A different model
 * selection replaces the worker, so at most one model stays resident. A worker
 * that fails is evicted and the next dictation starts clean.
 */
type Loader = NonNullable<
	NonNullable<ConstructorParameters<typeof Recognizer>[0]>["loadMoonshineHost"]
>;
export type MoonshineHost = Awaited<ReturnType<Loader>>;
type TranscriberConfig = Parameters<MoonshineHost["loadTranscriber"]>[0];

/** Same contract as the SDK's own loader; the runtime must be same-origin. */
export async function loadMoonshineWorkerHost(
	runtimeBaseUrl: string,
): Promise<MoonshineHost> {
	if (
		!globalThis.isSecureContext ||
		!globalThis.crossOriginIsolated ||
		typeof Worker === "undefined"
	) {
		throw new Error(
			"Local speech requires HTTPS and cross-origin isolation (COOP/COEP). No audio was sent to a server.",
		);
	}
	const base = new URL(runtimeBaseUrl, location.href);
	if (base.origin !== location.origin)
		throw new Error(
			"Moonshine runtime assets must be hosted on the application origin",
		);
	const moduleUrl = new URL("stt-worker-host.js", base).href;
	const module = await import(/* @vite-ignore */ moduleUrl);
	return new module.SttWorkerHost(base.href) as MoonshineHost;
}

interface PooledWorker {
	runtimeBaseUrl: string;
	host: Promise<MoonshineHost>;
	/** Model selection loaded into the worker, and its in-flight load. */
	model?: { key: string; loaded: Promise<void> };
	broken: boolean;
}

export class MoonshineHostPool {
	private worker: PooledWorker | null = null;
	private streams = 0;

	constructor(private readonly spawn: Loader = loadMoonshineWorkerHost) {}

	/** Loader to hand to `new Recognizer({ loadMoonshineHost })`. */
	readonly loader: Loader = async (runtimeBaseUrl) =>
		this.lease(runtimeBaseUrl);

	/** Terminate the shared worker (tests, settings reset). */
	dispose(): void {
		const worker = this.worker;
		this.worker = null;
		void worker?.host.then((host) => host.close()).catch(() => undefined);
	}

	private evict(worker: PooledWorker) {
		worker.broken = true;
		if (this.worker === worker) this.dispose();
	}

	private current(runtimeBaseUrl: string): PooledWorker {
		if (
			!this.worker ||
			this.worker.broken ||
			this.worker.runtimeBaseUrl !== runtimeBaseUrl
		) {
			this.dispose();
			const host = this.spawn(runtimeBaseUrl);
			const worker: PooledWorker = { runtimeBaseUrl, host, broken: false };
			this.worker = worker;
			host.catch(() => this.evict(worker));
		}
		return this.worker;
	}

	private async lease(runtimeBaseUrl: string): Promise<MoonshineHost> {
		let worker = this.current(runtimeBaseUrl);
		let host = await worker.host;
		const streamId = `oqto-${++this.streams}`;
		const pool = this;
		let closed = false;
		let created = false;
		let progress: MoonshineHost["onProgress"];
		const guard = <T>(work: Promise<T>): Promise<T> => {
			const owner = worker;
			return work.catch((error) => {
				pool.evict(owner);
				throw error;
			});
		};
		const lease: MoonshineHost = {
			get onProgress() {
				return progress;
			},
			set onProgress(handler) {
				progress = handler;
				host.onProgress = handler;
			},
			async loadTranscriber(config: TranscriberConfig) {
				const key = JSON.stringify([config.modelArch, config.source]);
				if (worker.model && worker.model.key !== key) {
					// Another language/model is resident: replace the worker so only
					// one model stays loaded. Streams are created after this, so the
					// lease can still move.
					pool.evict(worker);
					worker = pool.current(runtimeBaseUrl);
					host = await worker.host;
					host.onProgress = progress;
				}
				if (!worker.model) {
					const owner = worker;
					const loaded = guard(host.loadTranscriber(config));
					owner.model = { key, loaded };
					loaded.catch(() => {
						if (owner.model?.key === key) owner.model = undefined;
					});
				}
				return worker.model?.loaded;
			},
			async createStream(transcriberId, _streamId, options) {
				await guard(host.createStream(transcriberId, streamId, options));
				created = true;
			},
			setListener(_streamId, listener) {
				host.setListener(streamId, listener);
			},
			start() {
				return guard(host.start(streamId));
			},
			addAudio(_streamId, audio, sampleRate) {
				host.addAudio(streamId, audio, sampleRate);
			},
			queuedSeconds() {
				return host.queuedSeconds(streamId);
			},
			stop() {
				return guard(host.stop(streamId));
			},
			/** Ends this recognition's stream; the worker and model stay loaded. */
			close() {
				if (closed) return;
				closed = true;
				if (host.onProgress === progress) host.onProgress = undefined;
				// Cancelled while the model loaded: there is no stream to close.
				if (!created) return;
				const owner = worker;
				const closeStream = (
					host as MoonshineHost & {
						closeStream?: (id: string) => Promise<void>;
					}
				).closeStream;
				if (!closeStream) {
					pool.evict(owner);
					return;
				}
				void closeStream.call(host, streamId).catch(() => pool.evict(owner));
			},
		};
		return lease;
	}
}

/** Process-wide pool: one warm local speech model per page. */
export const moonshineHostPool = new MoonshineHostPool();
