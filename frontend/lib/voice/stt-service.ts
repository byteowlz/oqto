import {
	type RecognitionEvent,
	type RecognitionLanguage,
	type RecognitionProvider,
	Recognizer,
} from "@byteowlz/ears-browser";

export interface MicrophoneDevice {
	deviceId: string;
	label: string;
	isDefault: boolean;
}
export interface STTRecognitionSettings {
	provider: RecognitionProvider;
	language: RecognitionLanguage;
	runtimeBaseUrl?: string;
}
export interface STTCallbacks {
	onPreview?: (text: string) => void;
	/** Legacy voice-mode callback: emitted only for completed words. */
	onWord?: (word: string) => void;
	onFinal?: (text: string) => void;
	onError?: (error: string) => void;
	onVadProgress?: (progress: number) => void;
	onConnectionChange?: (connected: boolean) => void;
	onPreparation?: (label: string | null) => void;
}

/** Oqto owns capture/permissions/visualization; eaRS owns inference and finality. */
export class STTService {
	private recognizer = new Recognizer();
	private callbacks: STTCallbacks = {};
	private audioContext: AudioContext | null = null;
	private mediaStream: MediaStream | null = null;
	private workletNode: AudioWorkletNode | null = null;
	private source: MediaStreamAudioSourceNode | null = null;
	private analyserNode: AnalyserNode | null = null;
	private setupToken = 0;
	private ready = false;
	private listening = false;
	private selectedDeviceId: string | null = null;
	private preview = new Map<string, string>();
	private chunks: Float32Array[] = [];

	constructor(
		private wsUrl: string,
		_vadSilenceTimeoutMs = 1500,
		private selection: STTRecognitionSettings = {
			provider: "ears",
			language: "en",
		},
	) {
		this.recognizer.subscribe((event) => this.receive(event));
	}
	private receive(event: RecognitionEvent) {
		switch (event.type) {
			case "state":
				this.ready = event.state === "ready";
				this.callbacks.onConnectionChange?.(this.ready);
				this.callbacks.onPreparation?.(
					event.state === "preparing" ? "Preparing speech recognition…" : null,
				);
				break;
			case "progress":
				this.callbacks.onPreparation?.(
					event.total
						? `Loading speech model: ${Math.round((event.loaded / event.total) * 100)}%`
						: "Loading speech model…",
				);
				break;
			case "preview":
				this.preview.set(event.id, event.text);
				this.callbacks.onPreview?.(this.getCurrentTranscript());
				break;
			case "completed":
				this.preview.delete(event.id);
				this.callbacks.onPreview?.(this.getCurrentTranscript());
				if (event.text.trim()) {
					for (const word of event.text.split(/\s+/))
						this.callbacks.onWord?.(word);
					this.callbacks.onFinal?.(event.text);
				}
				break;
			case "error":
				this.releaseCapture();
				this.callbacks.onError?.(event.message);
				break;
		}
	}
	async connect() {
		if (this.ready) return;
		if (!["moonshine", "ears"].includes(this.selection.provider))
			throw new Error("Unsupported recognition provider");
		const sampleRate = 24000;
		await this.recognizer.prepare(
			this.selection.provider === "moonshine"
				? {
						provider: "moonshine",
						language: this.selection.language,
						sampleRate,
						runtimeBaseUrl:
							this.selection.runtimeBaseUrl ??
							`${import.meta.env.BASE_URL}speech/moonshine-0.1.5/`,
					}
				: {
						provider: "ears",
						language: this.selection.language,
						sampleRate,
						url: this.wsUrl,
					},
		);
	}
	async startListening() {
		if (this.listening) return;
		const token = ++this.setupToken;
		if (!window.isSecureContext || !navigator.mediaDevices?.getUserMedia) {
			throw new Error("Microphone capture requires HTTPS or localhost");
		}
		await this.connect();
		if (token !== this.setupToken) return;
		const stream = await navigator.mediaDevices.getUserMedia({
			audio: {
				channelCount: 1,
				echoCancellation: true,
				noiseSuppression: true,
				autoGainControl: true,
				...(this.selectedDeviceId && this.selectedDeviceId !== "default"
					? { deviceId: { exact: this.selectedDeviceId } }
					: {}),
			},
		});
		if (token !== this.setupToken) {
			for (const track of stream.getTracks()) track.stop();
			return;
		}
		this.mediaStream = stream;
		try {
			await this.attachCapture(stream, token);
		} catch (error) {
			this.stopListening();
			throw error;
		}
	}
	private async attachCapture(stream: MediaStream, token: number) {
		const context = new AudioContext({ sampleRate: 24000 });
		this.audioContext = context;
		if (context.sampleRate !== 24000) {
			this.stopListening();
			throw new Error("Browser did not provide a 24 kHz capture context");
		}
		const url = URL.createObjectURL(
			new Blob(
				[
					`
			class EarsCapture extends AudioWorkletProcessor {
				process(inputs) {
					if (inputs[0]?.[0]) this.port.postMessage(inputs[0][0]);
					return true;
				}
			}
			registerProcessor('ears-capture', EarsCapture);
		`,
				],
				{ type: "application/javascript" },
			),
		);
		try {
			await context.audioWorklet.addModule(url);
		} finally {
			URL.revokeObjectURL(url);
		}
		if (token !== this.setupToken) {
			await context
				.close()
				.catch((error) =>
					console.warn("[STT] Audio context cleanup failed", error),
				);
			return;
		}
		const worklet = new AudioWorkletNode(context, "ears-capture");
		this.workletNode = worklet;
		const source = context.createMediaStreamSource(stream);
		this.source = source;
		const analyser = context.createAnalyser();
		this.analyserNode = analyser;
		analyser.fftSize = 256;
		worklet.port.onmessage = (event: MessageEvent<Float32Array>) => {
			if (!this.listening) return;
			this.chunks.push(event.data);
			if (this.chunks.length >= 8) {
				try {
					this.flushAudio();
				} catch (error) {
					// Recognizer has already emitted the failure and stopped capture.
					console.warn("[STT] Audio batch could not be recognized", error);
				}
			}
		};
		source.connect(analyser);
		analyser.connect(worklet);
		worklet.connect(context.destination);
		this.listening = true;
		await context.resume();
	}
	private flushAudio() {
		if (!this.chunks.length) return;
		const audio = new Float32Array(
			this.chunks.reduce((sum, chunk) => sum + chunk.length, 0),
		);
		let offset = 0;
		for (const chunk of this.chunks) {
			audio.set(chunk, offset);
			offset += chunk.length;
		}
		this.chunks = [];
		this.recognizer.feed(audio);
	}
	private releaseCapture() {
		this.setupToken++;
		this.listening = false;
		if (this.workletNode) {
			this.workletNode.port.onmessage = null;
			this.workletNode.disconnect();
		}
		this.source?.disconnect();
		this.analyserNode?.disconnect();
		for (const track of this.mediaStream?.getTracks() ?? []) track.stop();
		if (this.audioContext?.state !== "closed")
			void this.audioContext
				?.close()
				.catch((error) =>
					console.warn("[STT] Audio context cleanup failed", error),
				);
		this.audioContext = null;
		this.mediaStream = null;
		this.source = null;
		this.workletNode = null;
		this.analyserNode = null;
	}
	/** Drain actual engine finality, never promote a preview. */
	async finishListening() {
		try {
			this.flushAudio();
			this.releaseCapture();
			await this.recognizer.finish();
		} finally {
			this.clearPreview();
		}
	}
	private clearPreview() {
		this.preview.clear();
		this.callbacks.onPreview?.("");
	}
	stopListening() {
		this.releaseCapture();
		this.chunks = [];
		this.recognizer.cancel();
		this.clearPreview();
	}
	disconnect() {
		this.stopListening();
	}
	setCallbacks(callbacks: STTCallbacks) {
		this.callbacks = { ...this.callbacks, ...callbacks };
	}
	isConnected() {
		return this.ready;
	}
	getIsListening() {
		return this.listening;
	}
	getCurrentTranscript() {
		return [...this.preview.values()].filter(Boolean).join(" ");
	}
	getInputVolume() {
		if (!this.analyserNode || !this.listening) return 0;
		const data = new Uint8Array(this.analyserNode.frequencyBinCount);
		this.analyserNode.getByteFrequencyData(data);
		const volume =
			data.reduce((sum, value) => sum + value, 0) / data.length / 255;
		return volume < 0.01 ? 0 : volume;
	}
	async listMicrophones(): Promise<MicrophoneDevice[]> {
		const devices = await navigator.mediaDevices.enumerateDevices();
		return devices
			.filter((device) => device.kind === "audioinput")
			.map((device) => ({
				deviceId: device.deviceId,
				label: device.label || "Microphone",
				isDefault: device.deviceId === "default",
			}));
	}
	setMicrophone(deviceId: string) {
		this.selectedDeviceId = deviceId;
		this.stopListening();
	}
	getSelectedMicrophone() {
		return this.selectedDeviceId;
	}
}
