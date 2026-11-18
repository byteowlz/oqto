/** Streaming PCM playback. Completion means generation ended AND every source drained. */
export class TTSPlayback {
	private context: AudioContext | null = null;
	private sources = new Set<AudioBufferSourceNode>();
	private nextTime = 0;
	private ended = false;
	private resolve?: () => void;
	private reject?: (error: Error) => void;
	constructor(private onPlaying: () => void) {}

	async open() {
		this.context = new AudioContext();
		await this.context.resume();
	}
	begin() {
		this.ended = false;
		this.nextTime = 0;
		return new Promise<void>((resolve, reject) => {
			this.resolve = resolve;
			this.reject = reject;
		});
	}
	push(pcm: Float32Array, sampleRate: number, speed: number) {
		const context = this.context;
		if (!context || !this.resolve) return;
		if (
			!(pcm instanceof Float32Array) ||
			!pcm.length ||
			pcm.some((n) => !Number.isFinite(n))
		)
			throw new Error("Invalid Phonon PCM");
		if (this.nextTime - context.currentTime > 30)
			throw new Error("Speech playback queue exceeded 30 seconds");
		const buffer = context.createBuffer(1, pcm.length, sampleRate);
		buffer.copyToChannel(new Float32Array(pcm), 0);
		const source = context.createBufferSource();
		source.buffer = buffer;
		source.playbackRate.value = speed;
		source.connect(context.destination);
		const start = Math.max(context.currentTime + 0.01, this.nextTime);
		this.nextTime = start + buffer.duration / speed;
		if (this.sources.size === 0) this.onPlaying();
		this.sources.add(source);
		source.onended = () => {
			source.disconnect();
			this.sources.delete(source);
			this.checkDrained();
		};
		source.start(start);
	}
	finish() {
		this.ended = true;
		this.checkDrained();
	}
	private checkDrained() {
		if (!this.ended || this.sources.size) return;
		this.resolve?.();
		this.resolve = undefined;
		this.reject = undefined;
	}
	dispose() {
		this.reject?.(new DOMException("Playback stopped", "AbortError"));
		this.resolve = undefined;
		this.reject = undefined;
		for (const source of this.sources) {
			source.onended = null;
			try {
				source.stop();
			} catch {
				// An already-ended source needs only disconnection.
			}
			source.disconnect();
		}
		this.sources.clear();
		const context = this.context;
		this.context = null;
		void context
			?.close()
			.catch((error) => console.warn("[TTS] Audio cleanup failed", error));
	}
}
