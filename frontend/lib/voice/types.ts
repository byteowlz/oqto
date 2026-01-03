/**
 * Voice mode configuration from backend.
 * Returned by the /features endpoint when voice is enabled.
 */
export interface VoiceConfig {
	/** WebSocket URL for the eaRS STT service */
	stt_url: string;
	/** WebSocket URL for the kokorox TTS service */
	tts_url: string;
	/** VAD timeout in milliseconds */
	vad_timeout_ms: number;
	/** Default kokorox voice ID */
	default_voice: string;
	/** Default TTS speed (0.1 - 3.0) */
	default_speed: number;
	/** Enable auto language detection */
	auto_language_detect: boolean;
	/** Whether TTS is muted by default */
	tts_muted: boolean;
	/** Continuous conversation mode */
	continuous_mode: boolean;
	/** Default visualizer style ("orb" or "kitt") */
	default_visualizer: string;
	/** Minimum words to interrupt TTS (0 = disabled) */
	interrupt_word_count: number;
	/** Reset interrupt word count after this silence in ms (0 = disabled) */
	interrupt_backoff_ms: number;
	/** Per-visualizer voice/speed settings */
	visualizer_voices: Record<string, { voice: string; speed: number }>;
}

/**
 * Voice mode state for UI.
 */
export type VoiceState = "idle" | "listening" | "processing" | "speaking";

/**
 * Voice visualizer type.
 */
export type VisualizerType = "orb" | "kitt" | string;

/**
 * Voice mode settings that can be persisted.
 */
export interface VoiceSettings {
	/** Selected visualizer */
	visualizer: VisualizerType;
	/** TTS muted */
	muted: boolean;
	/** Continuous mode enabled */
	continuous: boolean;
	/** Selected voice ID */
	voice: string;
	/** Speech speed */
	speed: number;
	/** VAD timeout in ms */
	vadTimeoutMs: number;
	/** Selected microphone device ID */
	microphoneId?: string;
	/** Min words spoken to interrupt TTS (0 = disabled) */
	interruptWordCount: number;
	/** Reset word count after this silence in ms (0 = disabled) */
	interruptBackoffMs: number;
}

/**
 * Default voice settings.
 */
/** Per-visualizer voice settings */
export interface VisualizerVoiceSettings {
	/** Voice ID for this visualizer */
	voice: string;
	/** Speech speed for this visualizer */
	speed: number;
}

export const DEFAULT_VOICE_SETTINGS: VoiceSettings = {
	visualizer: "orb",
	muted: false,
	continuous: true,
	voice: "af_heart",
	speed: 1.0,
	vadTimeoutMs: 1500,
	interruptWordCount: 2,
	interruptBackoffMs: 5000,
};

/** Default per-visualizer voice settings */
export const DEFAULT_VISUALIZER_VOICES: Record<
	string,
	VisualizerVoiceSettings
> = {
	orb: { voice: "af_heart", speed: 1.0 },
	kitt: { voice: "am_michael", speed: 1.1 },
};
