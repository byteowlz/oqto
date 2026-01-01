/**
 * Speech-to-Text Service for eaRS WebSocket server.
 *
 * Handles real-time audio streaming from microphone to eaRS,
 * receiving word-by-word transcriptions with VAD (Voice Activity Detection).
 *
 * Audio Requirements:
 * - Format: 32-bit float PCM
 * - Sample Rate: 24000 Hz
 * - Channels: Mono
 */

/** Message types from eaRS server */
interface STTMessage {
	type: "word" | "final" | "pause" | "error" | "status" | "languagechanged";
	word?: string;
	text?: string;
	start_time?: number;
	end_time?: number;
	message?: string;
	timestamp?: number;
	paused?: boolean;
	vad?: boolean;
	vad_timeout?: number;
	lang?: string;
	words?: Array<{
		word: string;
		start_time: number;
		end_time?: number;
	}>;
}

/** Microphone device information */
export interface MicrophoneDevice {
	deviceId: string;
	label: string;
	isDefault: boolean;
}

/** STT Service event callbacks */
export interface STTCallbacks {
	onWord?: (word: string) => void;
	onFinal?: (text: string) => void;
	onError?: (error: string) => void;
	onVadProgress?: (progress: number) => void;
	onConnectionChange?: (connected: boolean) => void;
}

/**
 * STT Service for real-time speech-to-text via eaRS WebSocket.
 */
export class STTService {
	private ws: WebSocket | null = null;
	private audioContext: AudioContext | null = null;
	private mediaStream: MediaStream | null = null;
	private workletNode: AudioWorkletNode | null = null;
	private source: MediaStreamAudioSourceNode | null = null;
	private analyserNode: AnalyserNode | null = null;

	private callbacks: STTCallbacks = {};
	private vadTimeoutId: number | null = null;
	private vadSilenceTimeoutMs: number;
	private vadProgressIntervalId: number | null = null;
	private vadStartTime = 0;
	private currentTranscript = "";
	private isListening = false;
	private audioSetupToken = 0;
	private selectedDeviceId: string | null = null;

	constructor(
		private wsUrl: string,
		vadSilenceTimeoutMs = 1500,
	) {
		this.vadSilenceTimeoutMs = vadSilenceTimeoutMs;
	}

	/**
	 * Update VAD timeout dynamically.
	 */
	setVadTimeout(ms: number) {
		this.vadSilenceTimeoutMs = ms;
	}

	/**
	 * List available microphone devices.
	 */
	async listMicrophones(): Promise<MicrophoneDevice[]> {
		try {
			// Request permission first to get device labels
			if (!this.mediaStream) {
				const tempStream = await navigator.mediaDevices.getUserMedia({
					audio: true,
				});
				for (const track of tempStream.getTracks()) {
					track.stop();
				}
			}

			const devices = await navigator.mediaDevices.enumerateDevices();
			const audioInputs = devices.filter(
				(device) => device.kind === "audioinput",
			);

			return audioInputs.map((device) => ({
				deviceId: device.deviceId,
				label: device.label || `Microphone ${device.deviceId.substring(0, 8)}`,
				isDefault: device.deviceId === "default",
			}));
		} catch (error) {
			console.error("[STT] Failed to enumerate microphones:", error);
			throw error;
		}
	}

	/**
	 * Set the microphone device to use.
	 */
	setMicrophone(deviceId: string) {
		console.log("[STT] Setting microphone to:", deviceId);
		this.selectedDeviceId = deviceId;

		// Restart listening if currently active
		if (this.isListening) {
			this.stopListening();
			this.startListening().catch((error) => {
				console.error("[STT] Failed to restart with new microphone:", error);
				this.callbacks.onError?.(`Failed to switch microphone: ${error}`);
			});
		}
	}

	/**
	 * Get currently selected microphone device ID.
	 */
	getSelectedMicrophone(): string | null {
		return this.selectedDeviceId;
	}

	/**
	 * Connect to the eaRS WebSocket server.
	 */
	async connect(): Promise<void> {
		if (
			this.ws &&
			(this.ws.readyState === WebSocket.OPEN ||
				this.ws.readyState === WebSocket.CONNECTING)
		) {
			console.log("[STT] Already connected or connecting");
			return;
		}

		return new Promise((resolve, reject) => {
			try {
				console.log("[STT] Connecting to:", this.wsUrl);
				this.ws = new WebSocket(this.wsUrl);
				this.ws.binaryType = "arraybuffer";

				this.ws.onopen = () => {
					console.log("[STT] Connected to eaRS server");
					this.callbacks.onConnectionChange?.(true);
					resolve();
				};

				this.ws.onmessage = (event) => {
					try {
						const message: STTMessage = JSON.parse(event.data);
						this.handleMessage(message);
					} catch (error) {
						console.error("[STT] Failed to parse message:", error);
					}
				};

				this.ws.onerror = (error) => {
					console.error("[STT] WebSocket error:", error);
					reject(error);
				};

				this.ws.onclose = (event) => {
					console.log("[STT] WebSocket closed:", event.code, event.reason);
					this.callbacks.onConnectionChange?.(false);
					this.stopListening();
				};
			} catch (error) {
				reject(error);
			}
		});
	}

	private handleMessage(message: STTMessage) {
		switch (message.type) {
			case "word":
				if (message.word) {
					this.currentTranscript +=
						(this.currentTranscript ? " " : "") + message.word;
					this.resetVadTimeout();
					this.callbacks.onWord?.(message.word);
				}
				break;

			case "final":
				this.clearVadTimeout();
				if (message.text) {
					this.callbacks.onFinal?.(message.text);
				}
				this.currentTranscript = "";
				break;

			case "pause":
				// VAD detected silence - could trigger final if we have content
				console.log("[STT] VAD pause detected");
				break;

			case "error":
				console.error("[STT] Server error:", message.message);
				this.clearVadTimeout();
				this.callbacks.onError?.(message.message || "Unknown error");
				break;

			case "status":
				console.log("[STT] Server status:", message);
				break;

			case "languagechanged":
				console.log("[STT] Language changed to:", message.lang);
				break;
		}
	}

	private resetVadTimeout() {
		this.clearVadTimeout();
		this.vadStartTime = Date.now();

		// Set timeout for silence detection
		this.vadTimeoutId = window.setTimeout(() => {
			console.log("[STT] VAD timeout - silence detected");
			const finalTranscript = this.currentTranscript.trim();
			this.clearVadTimeout();

			if (finalTranscript) {
				this.callbacks.onFinal?.(finalTranscript);
			}
			this.currentTranscript = "";
		}, this.vadSilenceTimeoutMs);

		// Progress callback at ~60fps for smooth UI updates
		this.vadProgressIntervalId = window.setInterval(() => {
			const elapsed = Date.now() - this.vadStartTime;
			const progress = Math.min(1, elapsed / this.vadSilenceTimeoutMs);
			this.callbacks.onVadProgress?.(progress);
		}, 16);
	}

	private clearVadTimeout() {
		if (this.vadTimeoutId !== null) {
			clearTimeout(this.vadTimeoutId);
			this.vadTimeoutId = null;
		}
		if (this.vadProgressIntervalId !== null) {
			clearInterval(this.vadProgressIntervalId);
			this.vadProgressIntervalId = null;
		}
		this.callbacks.onVadProgress?.(0);
	}

	/**
	 * Start listening and streaming audio to eaRS.
	 */
	async startListening(): Promise<void> {
		console.log("[STT] Starting audio capture...");

		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
			throw new Error("WebSocket not connected");
		}

		// Check for secure context (HTTPS or localhost)
		if (!window.isSecureContext) {
			throw new Error(
				"Voice mode requires HTTPS. Please access the site via https:// or localhost",
			);
		}

		// Check for mediaDevices API
		if (!navigator.mediaDevices?.getUserMedia) {
			throw new Error(
				"getUserMedia not supported. Voice mode requires a modern browser with HTTPS",
			);
		}

		const setupToken = ++this.audioSetupToken;

		try {
			// Request microphone access
			const audioConstraints: MediaTrackConstraints = {
				channelCount: 1,
				sampleRate: 24000,
				echoCancellation: true,
				noiseSuppression: true,
				autoGainControl: true,
			};

			if (this.selectedDeviceId && this.selectedDeviceId !== "default") {
				audioConstraints.deviceId = { exact: this.selectedDeviceId };
			}

			this.mediaStream = await navigator.mediaDevices.getUserMedia({
				audio: audioConstraints,
			});
			console.log("[STT] Microphone access granted");
		} catch (error) {
			console.error("[STT] Microphone access error:", error);
			if (error instanceof Error) {
				switch (error.name) {
					case "NotAllowedError":
						throw new Error("Microphone permission denied");
					case "NotFoundError":
						throw new Error("No microphone found");
					case "OverconstrainedError":
						throw new Error("Selected microphone not available");
					default:
						break;
				}
			}
			throw error;
		}

		// Check if setup was cancelled
		if (setupToken !== this.audioSetupToken) {
			for (const track of this.mediaStream?.getTracks() ?? []) {
				track.stop();
			}
			this.mediaStream = null;
			return;
		}

		// Create AudioContext at 24kHz (eaRS requirement)
		const audioContext = new AudioContext({ sampleRate: 24000 });
		this.audioContext = audioContext;

		const source = audioContext.createMediaStreamSource(this.mediaStream);

		// Create analyser for volume visualization
		const analyserNode = audioContext.createAnalyser();
		analyserNode.fftSize = 256;
		analyserNode.smoothingTimeConstant = 0.8;

		// Create AudioWorklet for processing
		await audioContext.audioWorklet.addModule(
			URL.createObjectURL(
				new Blob(
					[
						`
        class AudioProcessor extends AudioWorkletProcessor {
          process(inputs) {
            const input = inputs[0];
            if (input.length > 0) {
              const samples = input[0];
              this.port.postMessage(samples);
            }
            return true;
          }
        }
        registerProcessor('audio-processor', AudioProcessor);
      `,
					],
					{ type: "application/javascript" },
				),
			),
		);

		// Check if setup was cancelled during worklet registration
		if (setupToken !== this.audioSetupToken) {
			await audioContext.close().catch(console.error);
			return;
		}

		const workletNode = new AudioWorkletNode(audioContext, "audio-processor");

		// Send audio chunks to WebSocket
		workletNode.port.onmessage = (e) => {
			if (!this.isListening) return;

			if (this.ws && this.ws.readyState === WebSocket.OPEN) {
				const samples = e.data;
				const buffer = new Float32Array(samples);
				try {
					this.ws.send(buffer.buffer);
				} catch (error) {
					console.error("[STT] Error sending audio:", error);
					this.stopListening();
				}
			}
		};

		// Check if setup was cancelled after worklet setup
		if (setupToken !== this.audioSetupToken) {
			workletNode.port.onmessage = null;
			workletNode.disconnect();
			await audioContext.close().catch(console.error);
			return;
		}

		// Store references and connect pipeline
		this.source = source;
		this.analyserNode = analyserNode;
		this.workletNode = workletNode;

		source.connect(analyserNode);
		analyserNode.connect(workletNode);
		workletNode.connect(audioContext.destination);
		this.isListening = true;

		console.log("[STT] Audio pipeline connected");
	}

	/**
	 * Stop listening and release audio resources.
	 */
	stopListening() {
		console.log("[STT] Stopping audio capture...");
		this.audioSetupToken++;
		this.isListening = false;
		this.clearVadTimeout();

		if (this.workletNode) {
			try {
				this.workletNode.port.onmessage = null;
				this.workletNode.disconnect();
			} catch (error) {
				console.error("[STT] Error disconnecting worklet:", error);
			}
			this.workletNode = null;
		}

		if (this.analyserNode) {
			try {
				this.analyserNode.disconnect();
			} catch (error) {
				console.error("[STT] Error disconnecting analyser:", error);
			}
			this.analyserNode = null;
		}

		if (this.source) {
			try {
				this.source.disconnect();
			} catch (error) {
				console.error("[STT] Error disconnecting source:", error);
			}
			this.source = null;
		}

		if (this.mediaStream) {
			try {
				for (const track of this.mediaStream.getTracks()) {
					track.stop();
				}
			} catch (error) {
				console.error("[STT] Error stopping media tracks:", error);
			}
			this.mediaStream = null;
		}

		if (this.audioContext && this.audioContext.state !== "closed") {
			this.audioContext.close().catch(console.error);
		}
		this.audioContext = null;

		console.log("[STT] Audio capture stopped");
	}

	/**
	 * Set event callbacks.
	 */
	setCallbacks(callbacks: STTCallbacks) {
		this.callbacks = { ...this.callbacks, ...callbacks };
	}

	/**
	 * Convenience methods for setting individual callbacks.
	 */
	onWord(callback: (word: string) => void) {
		this.callbacks.onWord = callback;
	}

	onFinal(callback: (text: string) => void) {
		this.callbacks.onFinal = callback;
	}

	onError(callback: (error: string) => void) {
		this.callbacks.onError = callback;
	}

	onVadProgress(callback: (progress: number) => void) {
		this.callbacks.onVadProgress = callback;
	}

	onConnectionChange(callback: (connected: boolean) => void) {
		this.callbacks.onConnectionChange = callback;
	}

	/**
	 * Disconnect from the WebSocket and cleanup all resources.
	 */
	disconnect() {
		console.log("[STT] Disconnecting...");
		this.isListening = false;
		this.clearVadTimeout();
		this.currentTranscript = "";
		this.stopListening();

		if (this.ws) {
			try {
				if (this.ws.readyState === WebSocket.OPEN) {
					this.ws.send(JSON.stringify({ type: "stop" }));
				}
				this.ws.close(1000, "Client disconnecting");
			} catch (error) {
				console.error("[STT] Error during disconnect:", error);
			}
			this.ws = null;
		}

		console.log("[STT] Disconnected");
	}

	/**
	 * Check if connected to eaRS server.
	 */
	isConnected(): boolean {
		return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
	}

	/**
	 * Check if currently listening/recording.
	 */
	getIsListening(): boolean {
		return this.isListening;
	}

	/**
	 * Get current input volume level (0-1) for visualization.
	 */
	getInputVolume(): number {
		if (!this.analyserNode || !this.isListening) {
			return 0;
		}

		const data = new Uint8Array(this.analyserNode.frequencyBinCount);
		this.analyserNode.getByteFrequencyData(data);

		let sum = 0;
		for (let i = 0; i < data.length; i++) {
			sum += data[i];
		}
		const volume = sum / data.length / 255;

		// Apply noise floor
		const noiseFloor = 0.01;
		if (volume < noiseFloor) {
			return 0;
		}

		return volume;
	}

	/**
	 * Get current transcript being accumulated.
	 */
	getCurrentTranscript(): string {
		return this.currentTranscript;
	}

	/**
	 * Send a control command to the server.
	 */
	sendCommand(command: object) {
		if (this.ws && this.ws.readyState === WebSocket.OPEN) {
			this.ws.send(JSON.stringify(command));
		}
	}

	/**
	 * Set the transcription language.
	 */
	setLanguage(lang: string) {
		this.sendCommand({ type: "setlanguage", lang });
	}

	/**
	 * Pause transcription.
	 */
	pause() {
		this.sendCommand({ type: "pause" });
	}

	/**
	 * Resume transcription.
	 */
	resume() {
		this.sendCommand({ type: "resume" });
	}
}
