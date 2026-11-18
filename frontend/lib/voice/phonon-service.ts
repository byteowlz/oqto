import {
	type PhononModelAssets,
	loadPhononModel,
	loadPhononWasm,
} from "./phonon-model";
import type {
	ReadAloudCallbacks,
	ReadAloudService,
} from "./read-aloud-service";
import { TTSPlayback } from "./tts-playback";
import type { TTSSettings } from "./tts-settings";

interface Pending {
	id: number;
	resolve: (value: Record<string, unknown>) => void;
	reject: (error: Error) => void;
	timer: ReturnType<typeof setTimeout>;
	onChunk?: (pcm: Float32Array) => void;
}
export interface PhononDependencies {
	loadModel?: typeof loadPhononModel;
	loadWasm?: typeof loadPhononWasm;
	createWorker?: (url: URL) => Worker;
	createPlayback?: (onPlaying: () => void) => TTSPlayback;
}

/** Version-specific adapter for the UNMODIFIED phonon-tts 0.4.0 worker protocol. */
export class PhononReadAloudService implements ReadAloudService {
	private worker?: Worker;
	private assets?: PhononModelAssets;
	private abort = new AbortController();
	private playback: TTSPlayback;
	private pending?: Pending;
	private nextId = 0;
	private ready = false;
	private busy = false;
	private preparation?: Promise<void>;
	private preparationSucceeded = false;
	constructor(
		private settings: TTSSettings["phonon"],
		private callbacks: ReadAloudCallbacks,
		private dependencies: PhononDependencies = {},
	) {
		this.playback = (
			dependencies.createPlayback ?? ((onPlaying) => new TTSPlayback(onPlaying))
		)(() => {
			if (!this.abort.signal.aborted) this.callbacks.onPlaying?.();
		});
	}
	prepare(): Promise<void> {
		// Retain a pending/failed preparation, including its original rejection.
		if (this.preparation && !this.preparationSucceeded) return this.preparation;
		if (this.abort.signal.aborted)
			return Promise.reject(new DOMException("Playback stopped", "AbortError"));
		this.preparation ??= this.initialize();
		return this.preparation;
	}
	private async initialize() {
		try {
			this.assertSelection();
			this.abort.signal.throwIfAborted();
			// Unlock playback during the originating user interaction, before model download.
			await this.playback.open();
			this.abort.signal.throwIfAborted();
			this.callbacks.onPreparation?.(
				"Downloading/verifying Phonon English model…",
			);
			const assets = await (this.dependencies.loadModel ?? loadPhononModel)(
				this.abort.signal,
				(progress) => {
					if (!this.abort.signal.aborted) this.callbacks.onDownload?.(progress);
				},
			);
			this.assets = assets;
			if (this.abort.signal.aborted) {
				assets.release();
				this.assets = undefined;
				this.abort.signal.throwIfAborted();
			}
			this.callbacks.onDownload?.(null);
			this.callbacks.onPreparation?.("Initializing Phonon English…");
			const base = new URL(
				`${import.meta.env.BASE_URL}speech/phonon-0.4.0/`,
				location.href,
			);
			const wasm = await (this.dependencies.loadWasm ?? loadPhononWasm)(
				base,
				this.abort.signal,
			);
			this.abort.signal.throwIfAborted();
			const url = new URL("worker.js", base);
			if (url.origin !== location.origin)
				throw new Error("Phonon runtime must be same-origin");
			this.worker = (
				this.dependencies.createWorker ??
				((workerUrl) => new Worker(workerUrl, { type: "module" }))
			)(url);
			this.worker.onmessage = ({ data }) => this.receive(data);
			this.worker.onerror = (event) => {
				event.preventDefault();
				this.fail(
					new Error(
						`Phonon worker failed: ${event.message || "could not start"}`,
					),
				);
			};
			this.worker.onmessageerror = () =>
				this.fail(new Error("Phonon worker message could not be decoded"));
			const files = assets.files;
			const result = await this.request(
				"init",
				{
					options: {
						...wasm,
						lang: "en",
						rewrites: "default",
						conditions: [],
						quant: "q8",
						device: "cpu",
						threads: 1,
						// Verified page-owned cache/Blob URLs: never ask the worker to persist arbitrary URLs.
						cache: false,
						preload: ["default"],
						model: {
							weights: { q8: files["model.q8.gguf"] },
							config: files["config.json"],
							tokenizer: files["tokenizer.json"],
							voices: { default: files["default-voice.safetensors"] },
						},
					},
				},
				180000,
				undefined,
				[wasm.wasmUrl, wasm.simdWasmUrl],
			);
			this.abort.signal.throwIfAborted();
			if (
				result.sampleRate !== 24000 ||
				result.device !== "cpu" ||
				result.threads !== 1 ||
				!Array.isArray(result.languages) ||
				result.languages.length !== 1 ||
				result.languages[0] !== "en"
			)
				throw new Error(
					"Phonon runtime does not match the pinned English CPU contract",
				);
			assets.release();
			this.assets = undefined;
			this.ready = true;
			this.callbacks.onPreparation?.(null);
			this.callbacks.onVoicesLoaded?.(["default"]);
			this.preparationSucceeded = true;
		} catch (error) {
			this.dispose();
			throw error;
		}
	}
	private assertSelection() {
		if (this.settings.language !== "en")
			throw new Error(
				"Phonon 31M supports English only; no fallback was selected",
			);
		if (this.settings.voice !== "default")
			throw new Error("Phonon supports only the pinned default voice");
		if (
			!Number.isFinite(this.settings.speed) ||
			this.settings.speed < 0.5 ||
			this.settings.speed > 2
		)
			throw new Error("Phonon playback speed must be between 0.5 and 2");
		if (!globalThis.isSecureContext || typeof Worker === "undefined")
			throw new Error("Phonon requires HTTPS/localhost and Web Workers");
	}
	private request(
		type: string,
		payload: Record<string, unknown>,
		timeout: number,
		onChunk?: Pending["onChunk"],
		transfer: Transferable[] = [],
	) {
		this.abort.signal.throwIfAborted();
		if (!this.worker || this.pending)
			throw new Error("Phonon is unavailable or busy");
		return new Promise<Record<string, unknown>>((resolve, reject) => {
			const id = ++this.nextId;
			const timer = setTimeout(
				() => this.fail(new Error("Phonon operation timed out")),
				timeout,
			);
			this.pending = { id, resolve, reject, timer, onChunk };
			try {
				this.worker?.postMessage({ type, id, ...payload }, transfer);
			} catch (error) {
				this.fail(error instanceof Error ? error : new Error(String(error)));
			}
		});
	}
	private receive(data: {
		type: string;
		id: number;
		pcm: Float32Array;
		value: Record<string, unknown>;
		message: string;
	}) {
		if (
			this.abort.signal.aborted ||
			!this.pending ||
			data.id !== this.pending.id
		)
			return;
		try {
			switch (data.type) {
				case "progress":
					return; // Download totals come from verified page-owned loading.
				case "chunk":
					this.pending.onChunk?.(data.pcm);
					return;
				case "result": {
					const pending = this.pending;
					this.pending = undefined;
					clearTimeout(pending.timer);
					pending.resolve(data.value);
					return;
				}
				case "error":
					throw new Error(data.message || "Phonon inference failed");
				default:
					throw new Error(`Unexpected Phonon worker message: ${data.type}`);
			}
		} catch (error) {
			this.fail(error instanceof Error ? error : new Error(String(error)));
		}
	}
	async speak(text: string) {
		this.abort.signal.throwIfAborted();
		if (!this.ready || this.busy)
			throw new Error("Phonon is not ready or already speaking");
		if (!text.trim()) return;
		this.busy = true;
		const drained = this.playback.begin();
		// Disposal can reject drainage while a generation request is still pending.
		void drained.catch(() => {});
		try {
			const result = await this.request(
				"generate",
				{
					text,
					voice: "default",
					temperature: 0.3,
					seed: BigInt("4242424242424242"),
				},
				180000,
				(pcm) => this.playback.push(pcm, 24000, this.settings.speed),
			);
			this.abort.signal.throwIfAborted();
			if (result.cancelled)
				throw new DOMException("Playback stopped", "AbortError");
			this.playback.finish();
			await drained;
		} catch (error) {
			this.dispose();
			throw error;
		} finally {
			this.busy = false;
		}
	}
	private fail(error: Error) {
		const pending = this.pending;
		this.pending = undefined;
		if (pending) {
			clearTimeout(pending.timer);
			pending.reject(error);
		}
		this.dispose();
	}
	dispose() {
		if (this.abort.signal.aborted) return;
		this.abort.abort();
		this.ready = false;
		if (this.pending) {
			const pending = this.pending;
			this.pending = undefined;
			clearTimeout(pending.timer);
			pending.reject(new DOMException("Playback stopped", "AbortError"));
			// Best-effort protocol cancellation; termination does not wait on cooperative WASM yield.
			try {
				this.worker?.postMessage({ type: "cancel", id: pending.id });
			} catch {
				/* Termination remains authoritative. */
			}
		}
		if (this.worker) {
			this.worker.onmessage = null;
			this.worker.onerror = null;
			this.worker.onmessageerror = null;
			this.worker.terminate();
			this.worker = undefined;
		}
		this.assets?.release();
		this.assets = undefined;
		this.playback.dispose();
	}
}
