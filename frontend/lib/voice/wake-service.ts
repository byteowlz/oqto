/**
 * Wake phrases (waqy, oqto-623a). Shared by the legacy UI and OqtoUI.
 *
 * - Off by default; the caller opts in. The operator must also configure a
 *   release folder, which the server verifies before serving it.
 * - The microphone is opened only while armed, and closed again on disarm.
 * - Half-duplex: `setPlayback(true)` while the app speaks mutes detection,
 *   including a short echo tail after playback ends.
 * - A detection only runs the manual control the user chose (start dictation).
 *   It is evidence that a phrase was probably spoken: never authentication,
 *   identity or permission (waqy ADR 0001).
 */
import { WakeEngine } from "@/vendor/waqy-browser/waqy-engine.mjs";
import { onAppPlayback } from "./playback-gate";
// The vendored engine is plain JS; the service only relies on this subset.
type WakeEngineFactory = (
	ort: unknown,
	models: unknown,
	options: { threshold: number },
) => Promise<WakeEngineLike>;

export const WAKE_SAMPLE_RATE = 16000;
const CAPTURE_WORKLET = new URL(
	"./capture-worklet.js?no-inline",
	import.meta.url,
).href;
/** Manifest hash check is defence in depth; the server already refuses a bad release. */
const MAX_RELEASE_BYTES = 64 * 1024 * 1024;

export interface WakeManifestFile {
	sha256: string;
}
export interface WakeManifest {
	release: string;
	threshold: number;
	classifiers: string[];
	files: Record<string, WakeManifestFile>;
}
export interface WakeReleaseLocation {
	manifestUrl: string;
	filesUrl: string;
}
export interface WakeCallbacks {
	onState?: (state: WakeState) => void;
	onScores?: (scores: Record<string, number>) => void;
	onDetection?: (phrase: string, score: number) => void;
	onError?: (message: string) => void;
}
export type WakeState =
	| "off"
	| "loading"
	| "listening"
	| "muted"
	| "unavailable"
	| "error";

/** Everything the service needs from the platform, so tests never touch audio. */
export interface WakeDeps {
	loadManifest(url: string): Promise<WakeManifest>;
	loadFile(url: string): Promise<Uint8Array>;
	createEngine(
		models: {
			mel: Uint8Array;
			embedding: Uint8Array;
			classifiers: Record<string, Uint8Array>;
		},
		options: { threshold: number },
	): Promise<WakeEngineLike>;
	openCapture?(
		sampleRate: number,
		onSamples: (pcm: Float32Array) => void,
	): Promise<WakeCapture>;
	digest?(bytes: Uint8Array): Promise<string>;
}
/** Subset of the vendored engine this service uses. */
export interface WakeEngineLike {
	push(
		samples: Float32Array,
	): Promise<{ scores: Record<string, number>; detections: string[] }[]>;
	setPlayback(active: boolean): void;
	reset(): void;
}
export interface WakeCapture {
	/** Resolves with the track's current mute state after the change. */
	setEnabled(enabled: boolean): Promise<void>;
	close(): Promise<void>;
}

export function defaultDeps(): WakeDeps {
	return {
		async loadManifest(url) {
			const response = await fetch(url, { credentials: "same-origin" });
			if (!response.ok)
				throw new Error(`wake manifest HTTP ${response.status}`);
			return (await response.json()) as WakeManifest;
		},
		async loadFile(url) {
			const response = await fetch(url, { credentials: "same-origin" });
			if (!response.ok) throw new Error(`wake file HTTP ${response.status}`);
			return new Uint8Array(await response.arrayBuffer());
		},
		async createEngine(models, options) {
			const ort = await import("onnxruntime-web");
			ort.env.wasm.wasmPaths = `${import.meta.env.BASE_URL}speech/ort/`;
			// Single-threaded WASM: no SharedArrayBuffer/COOP requirement, predictable cost.
			ort.env.wasm.numThreads = 1;
			return (WakeEngine.create as unknown as WakeEngineFactory)(ort, models, {
				threshold: options.threshold,
			});
		},
		async openCapture(sampleRate, onSamples) {
			const stream = await navigator.mediaDevices.getUserMedia({
				audio: {
					channelCount: 1,
					echoCancellation: true,
					noiseSuppression: true,
					autoGainControl: true,
				},
			});
			const context = new AudioContext({ sampleRate });
			try {
				await context.audioWorklet.addModule(CAPTURE_WORKLET);
				const node = new AudioWorkletNode(context, "ears-capture");
				node.port.onmessage = (event) => onSamples(event.data as Float32Array);
				context.createMediaStreamSource(stream).connect(node);
				// Keep the graph running without routing the microphone to the speakers.
				const sink = context.createGain();
				sink.gain.value = 0;
				node.connect(sink).connect(context.destination);
				await context.resume();
				return {
					async setEnabled(enabled) {
						for (const track of stream.getAudioTracks())
							track.enabled = enabled;
					},
					async close() {
						for (const track of stream.getTracks()) track.stop();
						node.port.onmessage = null;
						node.disconnect();
						await context.close();
					},
				};
			} catch (error) {
				for (const track of stream.getTracks()) track.stop();
				await context.close();
				throw error;
			}
		},
		async digest(bytes) {
			const hash = await crypto.subtle.digest(
				"SHA-256",
				bytes as unknown as ArrayBuffer,
			);
			return [...new Uint8Array(hash)]
				.map((b) => b.toString(16).padStart(2, "0"))
				.join("");
		},
	};
}

/** Verify every served file against the manifest before handing bytes to the engine. */
export async function loadVerifiedRelease(
	location: WakeReleaseLocation,
	deps: WakeDeps = defaultDeps(),
): Promise<{
	manifest: WakeManifest;
	models: {
		mel: Uint8Array;
		embedding: Uint8Array;
		classifiers: Record<string, Uint8Array>;
	};
}> {
	const manifest = await deps.loadManifest(location.manifestUrl);
	const keys = ["mel", "embedding", ...manifest.classifiers];
	if (!manifest.release || !(manifest.threshold > 0 && manifest.threshold < 1))
		throw new Error("wake release manifest is invalid");
	if (!manifest.classifiers.length)
		throw new Error("wake release has no phrases");
	const bytes: Record<string, Uint8Array> = {};
	let total = 0;
	for (const key of keys) {
		const expected = manifest.files[key]?.sha256;
		if (!expected) throw new Error(`wake release has no file for '${key}'`);
		const data = await deps.loadFile(
			`${location.filesUrl}/${encodeURIComponent(key)}`,
		);
		total += data.byteLength;
		if (total > MAX_RELEASE_BYTES)
			throw new Error("wake release is larger than expected");
		if (deps.digest) {
			const actual = await deps.digest(data);
			if (actual.toLowerCase() !== expected.toLowerCase())
				throw new Error(
					`wake release file '${key}' does not match its manifest hash`,
				);
		}
		bytes[key] = data;
	}
	return {
		manifest,
		models: {
			mel: bytes.mel as Uint8Array,
			embedding: bytes.embedding as Uint8Array,
			classifiers: Object.fromEntries(
				manifest.classifiers.map((p) => [p, bytes[p] as Uint8Array]),
			),
		},
	};
}

export class WakeService {
	private engine: WakeEngineLike | null = null;
	private capture: WakeCapture | null = null;
	private manifest: WakeManifest | null = null;
	private state: WakeState = "off";
	private generation = 0;
	private playback = false;
	private cooldown = new Map<string, number>();
	private busy = Promise.resolve();
	private unlisten?: () => void;

	constructor(
		private readonly location: WakeReleaseLocation,
		private readonly callbacks: WakeCallbacks = {},
		private readonly deps: WakeDeps = defaultDeps(),
	) {}

	getState(): WakeState {
		return this.state;
	}
	getManifest(): WakeManifest | null {
		return this.manifest;
	}
	get phrases(): string[] {
		return this.manifest?.classifiers ?? [];
	}

	private setState(state: WakeState) {
		if (this.state === state) return;
		this.state = state;
		this.callbacks.onState?.(state);
	}

	/** Load the release and start listening. Idempotent; safe to call on enable. */
	async arm(): Promise<void> {
		if (this.capture || this.state === "loading") return;
		const token = ++this.generation;
		this.setState("loading");
		try {
			const { manifest, models } = await loadVerifiedRelease(
				this.location,
				this.deps,
			);
			if (token !== this.generation) return;
			this.manifest = manifest;
			this.engine ??= await this.deps.createEngine(models, {
				threshold: manifest.threshold,
			});
			if (token !== this.generation) return;
			this.engine.reset();
			if (!this.deps.openCapture) {
				this.setState("unavailable");
				return;
			}
			const capture = await this.deps.openCapture(WAKE_SAMPLE_RATE, (pcm) =>
				this.feed(pcm),
			);
			if (token !== this.generation) {
				await capture.close();
				return;
			}
			this.capture = capture;
			// App playback (TTS) mutes detection even when a host forgets setPlayback.
			this.unlisten?.();
			this.unlisten = onAppPlayback((playing) => this.setPlayback(playing));
			this.applyPlayback();
			this.setState(this.playback ? "muted" : "listening");
		} catch (error) {
			if (token !== this.generation) return;
			this.setState("error");
			this.callbacks.onError?.(message(error));
		}
	}

	/** Stop listening and close the microphone. Never throws. */
	async disarm(): Promise<void> {
		this.generation++;
		this.unlisten?.();
		this.unlisten = undefined;
		const capture = this.capture;
		this.capture = null;
		try {
			await capture?.close();
		} catch {
			// closing a device that already vanished is not an error worth surfacing
		}
		this.setState("off");
	}

	/** Half-duplex gate while the app speaks (and for the engine's echo tail after). */
	setPlayback(active: boolean): void {
		this.playback = active;
		this.applyPlayback();
		if (this.capture) this.setState(active ? "muted" : "listening");
	}

	private applyPlayback() {
		this.engine?.setPlayback(this.playback);
	}

	private feed(pcm: Float32Array) {
		if (!this.engine || this.playback) return;
		const engine = this.engine;
		this.busy = this.busy
			.then(async () => {
				for (const result of await engine.push(pcm)) {
					this.callbacks.onScores?.(result.scores);
					for (const phrase of result.detections)
						this.detect(phrase, result.scores[phrase] ?? 0);
				}
			})
			.catch((error) => {
				this.setState("error");
				this.callbacks.onError?.(message(error));
			});
	}

	private detect(phrase: string, score: number) {
		// A head that implies another one suppresses the shorter phrase (waqy ADR 0005).
		const now = Date.now();
		const until = this.cooldown.get(phrase) ?? 0;
		if (now < until) return;
		this.cooldown.set(phrase, now + 2000);
		this.callbacks.onDetection?.(phrase, score);
	}
}

function message(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}
