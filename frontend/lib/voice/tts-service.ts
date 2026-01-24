/**
 * Text-to-Speech Service for kokorox WebSocket server.
 *
 * Handles streaming text-to-speech synthesis with sentence-level audio chunks.
 * Audio is queued and played sequentially for seamless playback.
 */

/** Message types from kokorox server */
interface TTSMessage {
	type:
		| "voices"
		| "voice_changed"
		| "language_changed"
		| "speed_changed"
		| "auto_detect_changed"
		| "synthesis_started"
		| "audio_chunk"
		| "synthesis_completed"
		| "stream_started"
		| "stream_chunk"
		| "stream_ended"
		| "stream_cancelled"
		| "error";
	voice?: string;
	voices?: string[];
	chunk?: string; // Base64 encoded WAV
	index?: number;
	total?: number;
	total_chunks?: number;
	sample_rate?: number;
	message?: string;
	stream_id?: string;
}

/** TTS Service event callbacks */
export interface TTSCallbacks {
	onPlaying?: () => void;
	onStopped?: () => void;
	onChunk?: (index: number, total: number) => void;
	onVoicesLoaded?: (voices: string[], currentVoice: string) => void;
	onConnectionChange?: (connected: boolean) => void;
	onError?: (error: string) => void;
}

/**
 * TTS Service for real-time text-to-speech via kokorox WebSocket.
 */
export class TTSService {
	private ws: WebSocket | null = null;
	private audioContext: AudioContext | null = null;
	private audioQueue: AudioBuffer[] = [];
	private isPlaying = false;
	private currentVoice = "af_heart";
	private currentSpeed = 1.0;
	private availableVoices: string[] = [];
	private currentSource: AudioBufferSourceNode | null = null;
	private analyserNode: AnalyserNode | null = null;
	private isMuted = false;
	private stopTimer: number | null = null;
	private stopGraceMs = 150;

	private callbacks: TTSCallbacks = {};

	// Queue for managing multiple synthesis requests
	private synthesisQueue: Array<{
		text: string;
		resolve: () => void;
		reject: (error: Error) => void;
	}> = [];
	private isProcessing = false;

	// Flag to reject incoming audio after stopPlayback is called
	private isStopped = false;

	// Scheduled playback: track when the next audio should start
	private nextPlayTime = 0;
	// Track all scheduled sources for cleanup
	private scheduledSources: AudioBufferSourceNode[] = [];

	// Streaming mode: active stream ID (one stream per message)
	private activeStreamId: string | null = null;
	// Pending stream start promise
	private streamStartPromise: {
		resolve: (streamId: string) => void;
		reject: (error: Error) => void;
	} | null = null;

	constructor(private wsUrl: string) {}

	/**
	 * Connect to the kokorox WebSocket server.
	 */
	async connect(): Promise<void> {
		if (
			this.ws &&
			(this.ws.readyState === WebSocket.OPEN ||
				this.ws.readyState === WebSocket.CONNECTING)
		) {
			console.log("[TTS] Already connected or connecting");
			return;
		}

		return new Promise((resolve, reject) => {
			try {
				console.log("[TTS] Connecting to:", this.wsUrl);
				this.ws = new WebSocket(this.wsUrl);

				this.ws.onopen = () => {
					console.log("[TTS] Connected to kokorox server");
					this.callbacks.onConnectionChange?.(true);
					// Request voice list on connect
					this.ws?.send(JSON.stringify({ command: "list_voices" }));
					resolve();
				};

				this.ws.onmessage = (event) => {
					try {
						const message: TTSMessage = JSON.parse(event.data);
						this.handleMessage(message);
					} catch (error) {
						console.error("[TTS] Failed to parse message:", error);
					}
				};

				this.ws.onerror = (error) => {
					console.error("[TTS] WebSocket error:", error);
					reject(error);
				};

				this.ws.onclose = () => {
					console.log("[TTS] WebSocket closed");
					this.callbacks.onConnectionChange?.(false);
				};
			} catch (error) {
				reject(error);
			}
		});
	}

	private handleMessage(message: TTSMessage) {
		switch (message.type) {
			case "voices":
				if (message.voices) {
					this.availableVoices = message.voices;
					console.log("[TTS] Available voices:", this.availableVoices.length);
				}
				if (message.voice) {
					this.currentVoice = message.voice;
					console.log("[TTS] Current voice:", this.currentVoice);
				}
				this.callbacks.onVoicesLoaded?.(
					this.availableVoices,
					this.currentVoice,
				);
				break;

			case "voice_changed":
				if (message.voice) {
					this.currentVoice = message.voice;
					console.log("[TTS] Voice changed to:", this.currentVoice);
				}
				break;

			case "speed_changed":
				console.log("[TTS] Speed changed");
				break;

			case "language_changed":
				console.log("[TTS] Language changed");
				break;

			case "auto_detect_changed":
				console.log("[TTS] Auto-detect changed");
				break;

			case "synthesis_started":
				console.log("[TTS] Synthesis started");
				break;

			case "audio_chunk":
				if (message.chunk) {
					console.log("[TTS] Audio chunk", message.index, "of", message.total);
					this.callbacks.onChunk?.(message.index || 0, message.total || 1);
					if (!this.isMuted) {
						this.handleAudioChunk(message.chunk);
					}
				}
				break;

			case "synthesis_completed":
				console.log("[TTS] Synthesis completed");
				// Resolve current synthesis promise
				if (this.synthesisQueue.length > 0) {
					const item = this.synthesisQueue.shift();
					item?.resolve();
				}
				this.isProcessing = false;
				this.processNextInQueue();
				break;

			// Streaming mode messages
			case "stream_started":
				if (message.stream_id) {
					console.log("[TTS] Stream started:", message.stream_id);
					this.activeStreamId = message.stream_id;
					this.streamStartPromise?.resolve(message.stream_id);
					this.streamStartPromise = null;
				}
				break;

			case "stream_chunk":
				if (message.chunk && message.stream_id === this.activeStreamId) {
					console.log("[TTS] Stream chunk", message.index);
					this.callbacks.onChunk?.(message.index || 0, -1); // -1 = unknown total
					if (!this.isMuted) {
						this.handleAudioChunk(message.chunk);
					}
				}
				break;

			case "stream_ended":
				if (message.stream_id === this.activeStreamId) {
					console.log("[TTS] Stream ended, total chunks:", message.total_chunks);
					this.activeStreamId = null;
				}
				break;

			case "stream_cancelled":
				if (message.stream_id === this.activeStreamId) {
					console.log("[TTS] Stream cancelled");
					this.activeStreamId = null;
				}
				break;

			case "error":
				console.error("[TTS] Synthesis error:", message.message);
				this.callbacks.onError?.(message.message || "Synthesis failed");
				this.isProcessing = false;
				// Also reject pending stream start
				this.streamStartPromise?.reject(new Error(message.message || "TTS error"));
				this.streamStartPromise = null;
				if (this.synthesisQueue.length > 0) {
					const item = this.synthesisQueue.shift();
					item?.reject(new Error(message.message || "TTS synthesis failed"));
				}
				this.processNextInQueue();
				break;
		}
	}

	private async handleAudioChunk(base64Chunk: string) {
		// Reject chunks if playback was stopped
		if (this.isStopped) {
			console.log("[TTS] Ignoring audio chunk - playback stopped");
			return;
		}

		try {
			// Create AudioContext on first chunk (user interaction required)
			if (!this.audioContext) {
				const AudioContextCtor =
					window.AudioContext ||
					(window as Window & { webkitAudioContext?: typeof AudioContext })
						.webkitAudioContext;
				if (!AudioContextCtor) {
					throw new Error("AudioContext is not supported in this browser");
				}
				this.audioContext = new AudioContextCtor();
				console.log(
					"[TTS] AudioContext created, sample rate:",
					this.audioContext.sampleRate,
				);

				// Create analyser for output volume visualization
				this.analyserNode = this.audioContext.createAnalyser();
				this.analyserNode.fftSize = 256;
				this.analyserNode.smoothingTimeConstant = 0.5;
				this.analyserNode.connect(this.audioContext.destination);
			}

			// Resume if suspended (browser autoplay policy)
			if (this.audioContext.state === "suspended") {
				await this.audioContext.resume();
			}

			// Decode base64 audio
			const binaryString = atob(base64Chunk);
			const bytes = new Uint8Array(binaryString.length);
			for (let i = 0; i < binaryString.length; i++) {
				bytes[i] = binaryString.charCodeAt(i);
			}

			const audioBuffer = await this.audioContext.decodeAudioData(bytes.buffer);
			if (this.stopTimer) {
				window.clearTimeout(this.stopTimer);
				this.stopTimer = null;
			}

			// Schedule this chunk immediately for seamless playback
			this.scheduleAudioBuffer(audioBuffer);
		} catch (error) {
			console.error("[TTS] Failed to decode audio chunk:", error);
		}
	}

	/**
	 * Schedule an audio buffer for seamless playback.
	 * Uses precise timing to eliminate gaps between chunks.
	 */
	private scheduleAudioBuffer(audioBuffer: AudioBuffer) {
		const audioContext = this.audioContext;
		if (!audioContext) {
			return;
		}

		const source = audioContext.createBufferSource();
		source.buffer = audioBuffer;

		// Connect through analyser for volume visualization
		if (this.analyserNode) {
			source.connect(this.analyserNode);
		} else {
			source.connect(audioContext.destination);
		}

		// Schedule at the next available time slot (seamless with previous chunk)
		const now = audioContext.currentTime;
		const startTime = Math.max(now + 0.01, this.nextPlayTime); // Small buffer to avoid underrun
		this.nextPlayTime = startTime + audioBuffer.duration;

		// Track for cleanup
		this.scheduledSources.push(source);
		this.currentSource = source;

		// Update playing state
		if (!this.isPlaying) {
			this.isPlaying = true;
			this.callbacks.onPlaying?.();
		}

		// Clean up finished sources from tracking array
		source.onended = () => {
			const idx = this.scheduledSources.indexOf(source);
			if (idx !== -1) {
				this.scheduledSources.splice(idx, 1);
			}
			// If this was the last source, schedule stop check
			if (this.scheduledSources.length === 0) {
				this.scheduleStopCheck();
			}
		};

		source.start(startTime);
	}

	private playNextChunk() {
		// Legacy method - now audio is scheduled immediately in handleAudioChunk
		// This is kept for compatibility but shouldn't be called
		if (this.audioQueue.length === 0) {
			this.scheduleStopCheck();
			return;
		}

		while (this.audioQueue.length > 0) {
			const audioBuffer = this.audioQueue.shift();
			if (audioBuffer) {
				this.scheduleAudioBuffer(audioBuffer);
			}
		}
	}

	private scheduleStopCheck() {
		if (this.stopTimer) {
			return;
		}
		this.stopTimer = window.setTimeout(() => {
			this.stopTimer = null;
			if (this.audioQueue.length > 0) {
				if (!this.isPlaying) {
					this.playNextChunk();
				}
				return;
			}
			if (this.isProcessing || this.synthesisQueue.length > 0) {
				this.scheduleStopCheck();
				return;
			}
			console.log("[TTS] Audio queue empty, playback finished");
			this.isPlaying = false;
			this.currentSource = null;
			this.callbacks.onStopped?.();
		}, this.stopGraceMs);
	}

	private processNextInQueue() {
		if (this.isProcessing || this.synthesisQueue.length === 0) {
			return;
		}

		this.isProcessing = true;
		this.isStopped = false; // Clear stopped flag when starting new synthesis
		const item = this.synthesisQueue[0];

		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
			console.error("[TTS] WebSocket not connected");
			this.synthesisQueue.shift();
			item.reject(new Error("TTS WebSocket not connected"));
			this.isProcessing = false;
			return;
		}

		console.log("[TTS] Synthesizing:", item.text.substring(0, 50));
		this.ws.send(
			JSON.stringify({
				command: "synthesize",
				text: item.text,
			}),
		);
	}

	/**
	 * Set event callbacks.
	 */
	setCallbacks(callbacks: TTSCallbacks) {
		this.callbacks = { ...this.callbacks, ...callbacks };
	}

	/**
	 * Convenience methods for setting individual callbacks.
	 */
	onPlaying(callback: () => void) {
		this.callbacks.onPlaying = callback;
	}

	onStopped(callback: () => void) {
		this.callbacks.onStopped = callback;
	}

	onChunk(callback: (index: number, total: number) => void) {
		this.callbacks.onChunk = callback;
	}

	onVoicesLoaded(callback: (voices: string[], currentVoice: string) => void) {
		this.callbacks.onVoicesLoaded = callback;
	}

	onConnectionChange(callback: (connected: boolean) => void) {
		this.callbacks.onConnectionChange = callback;
	}

	onError(callback: (error: string) => void) {
		this.callbacks.onError = callback;
	}

	/**
	 * Synthesize and speak text.
	 * Returns a promise that resolves when synthesis is complete.
	 */
	async speak(text: string): Promise<void> {
		if (!text.trim()) return;

		return new Promise((resolve, reject) => {
			this.synthesisQueue.push({ text, resolve, reject });
			if (!this.isProcessing) {
				this.processNextInQueue();
			}
		});
	}

	// =========================================================================
	// Streaming API - for low-latency incremental TTS
	// =========================================================================

	/**
	 * Start a new streaming session for a message.
	 * Call streamAppend() to add text incrementally, then streamEnd() when done.
	 */
	async streamStart(): Promise<string> {
		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
			throw new Error("WebSocket not connected");
		}

		// Cancel any existing stream
		if (this.activeStreamId) {
			this.streamCancel();
		}

		this.isStopped = false;

		return new Promise((resolve, reject) => {
			this.streamStartPromise = { resolve, reject };
			this.ws?.send(
				JSON.stringify({
					command: "stream_start",
					voice: this.currentVoice,
					speed: this.currentSpeed,
				}),
			);
		});
	}

	/**
	 * Append text to the active stream.
	 * Kokorox will synthesize complete sentences and send audio immediately.
	 */
	streamAppend(text: string): void {
		if (!this.activeStreamId || !this.ws || this.ws.readyState !== WebSocket.OPEN) {
			console.warn("[TTS] No active stream to append to");
			return;
		}

		if (!text) return;

		this.ws.send(
			JSON.stringify({
				command: "stream_append",
				stream_id: this.activeStreamId,
				text,
			}),
		);
	}

	/**
	 * End the active stream, flushing any remaining text.
	 */
	streamEnd(): void {
		if (!this.activeStreamId || !this.ws || this.ws.readyState !== WebSocket.OPEN) {
			return;
		}

		this.ws.send(
			JSON.stringify({
				command: "stream_end",
				stream_id: this.activeStreamId,
			}),
		);
	}

	/**
	 * Cancel the active stream without flushing.
	 */
	streamCancel(): void {
		if (!this.activeStreamId || !this.ws || this.ws.readyState !== WebSocket.OPEN) {
			return;
		}

		this.ws.send(
			JSON.stringify({
				command: "stream_cancel",
				stream_id: this.activeStreamId,
			}),
		);
		this.activeStreamId = null;
	}

	/**
	 * Check if a stream is active.
	 */
	isStreaming(): boolean {
		return this.activeStreamId !== null;
	}

	/**
	 * Set the voice to use.
	 */
	async setVoice(voice: string): Promise<void> {
		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
			throw new Error("WebSocket not connected");
		}

		return new Promise((resolve, reject) => {
			const timeout = setTimeout(
				() => reject(new Error("Voice change timeout")),
				5000,
			);

			const ws = this.ws;
			if (!ws) {
				clearTimeout(timeout);
				reject(new Error("WebSocket not connected"));
				return;
			}
			const originalHandler = ws.onmessage;

			ws.onmessage = (event) => {
				const message: TTSMessage = JSON.parse(event.data);
				if (message.type === "voice_changed") {
					clearTimeout(timeout);
					ws.onmessage = originalHandler;
					resolve();
				} else if (message.type === "error") {
					clearTimeout(timeout);
					ws.onmessage = originalHandler;
					reject(new Error("Failed to set voice"));
				}
				if (originalHandler) originalHandler.call(ws, event);
			};

			ws.send(JSON.stringify({ command: "set_voice", voice }));
		});
	}

	/**
	 * Set the speech speed (0.1 - 3.0).
	 */
	async setSpeed(speed: number): Promise<void> {
		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
			throw new Error("WebSocket not connected");
		}

		this.currentSpeed = Math.max(0.1, Math.min(3.0, speed));
		this.ws.send(
			JSON.stringify({ command: "set_speed", speed: this.currentSpeed }),
		);
	}

	/**
	 * Set the language for synthesis.
	 */
	setLanguage(language: string) {
		if (this.ws && this.ws.readyState === WebSocket.OPEN) {
			this.ws.send(JSON.stringify({ command: "set_language", language }));
		}
	}

	/**
	 * Enable/disable auto language detection.
	 */
	setAutoDetect(enabled: boolean) {
		if (this.ws && this.ws.readyState === WebSocket.OPEN) {
			this.ws.send(
				JSON.stringify({ command: "set_auto_detect", auto_detect: enabled }),
			);
		}
	}

	/**
	 * Stop current playback, clear audio queue, and cancel pending synthesis.
	 * Returns any pending text that was not yet spoken.
	 */
	stopPlayback(): string[] {
		console.log("[TTS] Stopping playback");

		// Set stopped flag to reject any audio chunks that arrive after this
		this.isStopped = true;
		if (this.stopTimer) {
			window.clearTimeout(this.stopTimer);
			this.stopTimer = null;
		}

		// Stop all scheduled sources
		for (const source of this.scheduledSources) {
			try {
				source.stop();
				source.disconnect();
			} catch (error) {
				// Ignore errors from already-stopped sources
			}
		}
		this.scheduledSources = [];
		this.currentSource = null;

		// Reset scheduled playback time
		this.nextPlayTime = 0;

		this.audioQueue = [];
		this.isPlaying = false;

		// Collect pending text before clearing
		const pendingTexts: string[] = [];

		// Clear synthesis queue to stop pending TTS requests
		const pendingCount = this.synthesisQueue.length;
		if (pendingCount > 0) {
			console.log("[TTS] Clearing", pendingCount, "pending synthesis requests");
			for (const item of this.synthesisQueue) {
				pendingTexts.push(item.text);
				item.reject(new Error("Playback stopped"));
			}
			this.synthesisQueue = [];
		}
		this.isProcessing = false;

		this.callbacks.onStopped?.();

		return pendingTexts;
	}

	/**
	 * Set muted state (audio still synthesizes but doesn't play).
	 */
	setMuted(muted: boolean) {
		this.isMuted = muted;
		if (muted) {
			this.stopPlayback();
		}
	}

	/**
	 * Get muted state.
	 */
	getMuted(): boolean {
		return this.isMuted;
	}

	/**
	 * Disconnect from WebSocket and cleanup resources.
	 */
	disconnect() {
		console.log("[TTS] Disconnecting...");
		this.stopPlayback();

		if (this.ws) {
			this.ws.close();
			this.ws = null;
		}

		if (this.audioContext) {
			this.audioContext.close().catch(console.error);
			this.audioContext = null;
		}

		this.analyserNode = null;
		this.synthesisQueue = [];
		this.isProcessing = false;
		if (this.stopTimer) {
			window.clearTimeout(this.stopTimer);
			this.stopTimer = null;
		}

		console.log("[TTS] Disconnected");
	}

	/**
	 * Check if connected to kokorox server.
	 */
	isConnected(): boolean {
		return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
	}

	/**
	 * Check if currently playing audio.
	 */
	getIsPlaying(): boolean {
		return this.isPlaying;
	}

	/**
	 * Get current output volume level (0-1) for visualization.
	 */
	getOutputVolume(): number {
		if (!this.analyserNode || !this.isPlaying) {
			return 0;
		}

		const timeDomainData = new Uint8Array(this.analyserNode.fftSize);
		this.analyserNode.getByteTimeDomainData(timeDomainData);

		let sumSquares = 0;
		for (let i = 0; i < timeDomainData.length; i++) {
			const normalized = (timeDomainData[i] - 128) / 128;
			sumSquares += normalized * normalized;
		}

		const rms = Math.sqrt(sumSquares / timeDomainData.length);

		// Apply noise floor
		const noiseFloor = 0.008;
		if (rms < noiseFloor) {
			return 0;
		}

		// Scale and apply perceptual curve
		const adjusted = Math.max(0, rms - noiseFloor);
		let scaled = Math.min(1.0, adjusted * 10.0);
		scaled **= 0.7;

		return scaled;
	}

	/**
	 * Get available voices.
	 */
	getAvailableVoices(): string[] {
		return this.availableVoices;
	}

	/**
	 * Get current voice.
	 */
	getCurrentVoice(): string {
		return this.currentVoice;
	}

	/**
	 * Get current speed.
	 */
	getCurrentSpeed(): number {
		return this.currentSpeed;
	}
}
