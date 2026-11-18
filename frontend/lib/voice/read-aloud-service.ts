import { voiceProxyWsUrl } from "@/lib/api/files";
import type { TTSDownloadProgress } from "./phonon-model";
import { PhononReadAloudService } from "./phonon-service";
import { TTSService } from "./tts-service";
import type { TTSSettings } from "./tts-settings";

export interface ReadAloudCallbacks {
	onPlaying?: () => void;
	onVoicesLoaded?: (voices: string[]) => void;
	onPreparation?: (label: string | null) => void;
	onDownload?: (progress: TTSDownloadProgress | null) => void;
}
export interface ReadAloudService {
	prepare(): Promise<void>;
	/** Resolves after inference and scheduled Web Audio playback drain; not physical audibility. */
	speak(text: string): Promise<void>;
	/** Cancels preparation, inference, playback and pending requests. Not reusable. */
	dispose(): void;
}

/** Preserve the conversational TTSService API; add read-aloud-only drainage semantics. */
class KokoroxReadAloudService implements ReadAloudService {
	private service: TTSService;
	private closed = false;
	private rejectPending?: (error: Error) => void;
	private drained?: () => void;
	private failure: Promise<never>;
	private timer?: ReturnType<typeof setTimeout>;
	constructor(
		private settings: TTSSettings,
		callbacks: ReadAloudCallbacks,
	) {
		this.service = new TTSService(voiceProxyWsUrl("tts"));
		this.failure = new Promise<never>((_resolve, reject) => {
			this.rejectPending = reject;
		});
		void this.failure.catch(() => {});
		this.service.setCallbacks({
			onPlaying: () => {
				if (!this.closed) callbacks.onPlaying?.();
			},
			onStopped: () => this.drained?.(),
			onVoicesLoaded: (voices) => {
				if (!this.closed) callbacks.onVoicesLoaded?.(voices);
			},
			onError: (message) => this.fail(new Error(message)),
			onConnectionChange: (connected) => {
				if (!connected && !this.closed)
					this.fail(new Error("Kokorox disconnected"));
			},
		});
	}
	async prepare() {
		this.timer = setTimeout(
			() => this.fail(new Error("Kokorox preparation timed out")),
			15000,
		);
		try {
			await Promise.race([
				(async () => {
					await this.service.connect();
					if (this.closed)
						throw new DOMException("Playback stopped", "AbortError");
					await this.service.setVoice(this.settings.voice);
					await this.service.setSpeed(this.settings.speed);
				})(),
				this.failure,
			]);
		} catch (error) {
			this.dispose();
			throw error;
		} finally {
			clearTimeout(this.timer);
		}
	}
	async speak(text: string) {
		if (this.closed) throw new DOMException("Playback stopped", "AbortError");
		if (!text.trim()) return;
		if (this.drained) throw new Error("Kokorox is already speaking");
		const drained = new Promise<void>((resolve) => {
			this.drained = resolve;
		});
		this.timer = setTimeout(
			() => this.fail(new Error("Kokorox speech timed out")),
			180000,
		);
		try {
			await Promise.race([
				Promise.all([this.service.speak(text), drained]),
				this.failure,
			]);
		} finally {
			this.drained = undefined;
			clearTimeout(this.timer);
		}
	}
	private fail(error: Error) {
		this.rejectPending?.(error);
		this.dispose();
	}
	dispose() {
		if (this.closed) return;
		this.closed = true;
		clearTimeout(this.timer);
		this.rejectPending?.(new DOMException("Playback stopped", "AbortError"));
		this.drained = undefined;
		this.service.disconnect();
	}
}

export function createReadAloudService(
	settings: TTSSettings,
	callbacks: ReadAloudCallbacks,
): ReadAloudService {
	if (settings.provider === "phonon")
		return new PhononReadAloudService(settings.phonon, callbacks);
	if (settings.provider !== "kokorox")
		throw new Error("Unsupported read-aloud provider; select one explicitly");
	if (
		typeof settings.voice !== "string" ||
		!settings.voice ||
		!Number.isFinite(settings.speed) ||
		settings.speed < 0.5 ||
		settings.speed > 2
	)
		throw new Error("Invalid Kokorox voice or speed");
	return new KokoroxReadAloudService(settings, callbacks);
}
