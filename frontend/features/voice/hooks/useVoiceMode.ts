/**
 * Voice mode hook for managing STT/TTS integration in chat sessions.
 *
 * Handles:
 * - Connection to eaRS (STT) and kokorox (TTS) WebSocket servers
 * - Voice state machine (idle → listening → processing → speaking)
 * - Live transcription with VAD timeout
 * - Audio volume monitoring for visualizations
 * - Settings persistence
 */

import { voiceProxyWsUrl } from "@/lib/control-plane-client";
import { STTService, TTSService } from "@/lib/voice";
import type {
	VisualizerType,
	VisualizerVoiceSettings,
	VoiceConfig,
	VoiceSettings,
	VoiceState,
} from "@/lib/voice/types";
import {
	DEFAULT_VISUALIZER_VOICES,
	DEFAULT_VOICE_SETTINGS,
} from "@/lib/voice/types";
import { useCallback, useEffect, useRef, useState } from "react";

const VOICE_SETTINGS_KEY = "octo-voice-settings";
const VISUALIZER_VOICES_KEY = "octo-visualizer-voices";

/** Load voice settings from localStorage */
function loadVoiceSettings(): VoiceSettings {
	if (typeof window === "undefined") return DEFAULT_VOICE_SETTINGS;
	try {
		const stored = localStorage.getItem(VOICE_SETTINGS_KEY);
		if (stored) {
			return { ...DEFAULT_VOICE_SETTINGS, ...JSON.parse(stored) };
		}
	} catch (e) {
		console.error("[Voice] Failed to load settings:", e);
	}
	return DEFAULT_VOICE_SETTINGS;
}

/** Save voice settings to localStorage */
function saveVoiceSettings(settings: VoiceSettings) {
	if (typeof window === "undefined") return;
	try {
		localStorage.setItem(VOICE_SETTINGS_KEY, JSON.stringify(settings));
	} catch (e) {
		console.error("[Voice] Failed to save settings:", e);
	}
}

/** Load per-visualizer voice settings from localStorage */
function loadVisualizerVoices(): Record<string, VisualizerVoiceSettings> {
	if (typeof window === "undefined") return DEFAULT_VISUALIZER_VOICES;
	try {
		const stored = localStorage.getItem(VISUALIZER_VOICES_KEY);
		if (stored) {
			return { ...DEFAULT_VISUALIZER_VOICES, ...JSON.parse(stored) };
		}
	} catch (e) {
		console.error("[Voice] Failed to load visualizer voices:", e);
	}
	return DEFAULT_VISUALIZER_VOICES;
}

/** Save per-visualizer voice settings to localStorage */
function saveVisualizerVoices(voices: Record<string, VisualizerVoiceSettings>) {
	if (typeof window === "undefined") return;
	try {
		localStorage.setItem(VISUALIZER_VOICES_KEY, JSON.stringify(voices));
	} catch (e) {
		console.error("[Voice] Failed to save visualizer voices:", e);
	}
}

export interface UseVoiceModeOptions {
	/** Voice configuration from backend */
	config: VoiceConfig | null;
	/** Callback when final transcript is ready to send */
	onTranscript: (text: string) => void;
	/** Callback to get new assistant message text for TTS */
	getAssistantResponse?: () => string | null;
}

export interface UseVoiceModeReturn {
	// State
	/** Whether voice mode is active */
	isActive: boolean;
	/** Current voice state */
	voiceState: VoiceState;
	/** Live transcript being accumulated */
	liveTranscript: string;
	/** VAD progress (0-1) */
	vadProgress: number;
	/** Input volume (0-1) for visualization */
	inputVolume: number;
	/** Output volume (0-1) for visualization */
	outputVolume: number;
	/** Whether services are connected */
	isConnected: boolean;
	/** Error message if any */
	error: string | null;

	// Settings
	settings: VoiceSettings;
	setVisualizer: (type: VisualizerType) => void;
	setMuted: (muted: boolean) => void;
	setMicMuted: (muted: boolean) => void;
	setContinuous: (continuous: boolean) => void;
	setVoice: (voice: string) => void;
	setSpeed: (speed: number) => void;
	setVadTimeout: (ms: number) => void;
	setInterruptWordCount: (count: number) => void;
	availableVoices: string[];

	// Actions
	/** Start voice mode */
	start: () => Promise<void>;
	/** Stop voice mode */
	stop: () => void;
	/** Interrupt current TTS playback */
	interrupt: () => void;
	/** Speak text via TTS (for manual triggering) */
	speak: (text: string) => Promise<void>;

	// Streaming TTS (low-latency for LLM output)
	/** Start a new TTS stream for a message */
	streamStart: () => Promise<string>;
	/** Append text to the active stream */
	streamAppend: (text: string) => void;
	/** End the stream and flush remaining text */
	streamEnd: () => void;
	/** Cancel the stream without flushing */
	streamCancel: () => void;
	/** Check if a stream is active */
	isStreaming: boolean;
}

/**
 * Hook for managing voice mode in a chat session.
 */
export function useVoiceMode(options: UseVoiceModeOptions): UseVoiceModeReturn {
	const { config, onTranscript } = options;

	// Services
	const sttRef = useRef<STTService | null>(null);
	const ttsRef = useRef<TTSService | null>(null);

	// State
	const [isActive, setIsActive] = useState(false);
	const [voiceState, setVoiceState] = useState<VoiceState>("idle");
	const [liveTranscript, setLiveTranscript] = useState("");
	const [vadProgress, setVadProgress] = useState(0);
	const [inputVolume, setInputVolume] = useState(0);
	const [outputVolume, setOutputVolume] = useState(0);
	const [isConnected, setIsConnected] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [availableVoices, setAvailableVoices] = useState<string[]>([]);

	// Settings
	const [settings, setSettings] = useState<VoiceSettings>(loadVoiceSettings);
	const [visualizerVoices, setVisualizerVoices] =
		useState<Record<string, VisualizerVoiceSettings>>(loadVisualizerVoices);

	// Refs for animation loop
	const animationRef = useRef<number | null>(null);
	const smoothInputRef = useRef(0);
	const smoothOutputRef = useRef(0);

	// Track word count for interrupt-by-speaking
	const interruptWordCountRef = useRef(0);
	const interruptBackoffTimerRef = useRef<number | null>(null);
	const ttsStartedAtRef = useRef<number | null>(null);

	// Use word array instead of string concatenation for O(1) word accumulation
	const liveWordsRef = useRef<string[]>([]);

	// Apply config defaults on mount
	useEffect(() => {
		if (config) {
			setSettings((prev) => ({
				...prev,
				visualizer: config.default_visualizer as VisualizerType,
				muted: config.tts_muted,
				continuous: config.continuous_mode,
				voice: config.default_voice,
				speed: config.default_speed,
				vadTimeoutMs: config.vad_timeout_ms,
				interruptWordCount: config.interrupt_word_count ?? 2,
				interruptBackoffMs: config.interrupt_backoff_ms ?? 5000,
			}));

			// Apply config-based visualizer voices - config is source of truth,
			// localStorage only stores user overrides on top
			if (config.visualizer_voices) {
				setVisualizerVoices((_prev) => {
					// Start with config values
					const fromConfig: Record<string, VisualizerVoiceSettings> = {};
					for (const [vizId, vizConfig] of Object.entries(
						config.visualizer_voices,
					)) {
						fromConfig[vizId] = {
							voice: vizConfig.voice,
							speed: vizConfig.speed,
						};
					}
					// localStorage overrides are loaded via loadVisualizerVoices but we
					// want config to be authoritative - user can change in UI which saves to localStorage
					// For now, always use config values on load
					try {
						if (
							import.meta.env.DEV &&
							localStorage.getItem("debug:voice") === "1"
						) {
							console.debug(
								"[Voice] Applying config visualizer voices:",
								fromConfig,
							);
						}
					} catch {
						// ignore
					}
					return fromConfig;
				});
			}
		}
	}, [config]);

	// Save settings when they change
	useEffect(() => {
		saveVoiceSettings(settings);
	}, [settings]);

	// Save visualizer voices when they change
	useEffect(() => {
		saveVisualizerVoices(visualizerVoices);
	}, [visualizerVoices]);

	// Apply visualizer-specific voice settings when visualizer changes
	useEffect(() => {
		const vizVoice = visualizerVoices[settings.visualizer];
		const tts = ttsRef.current;
		if (vizVoice && tts?.isConnected()) {
			try {
				if (
					import.meta.env.DEV &&
					localStorage.getItem("debug:voice") === "1"
				) {
					console.debug(
						"[Voice] Switching to visualizer voice:",
						settings.visualizer,
						vizVoice,
					);
				}
			} catch {
				// ignore
			}

			// Stop current playback and capture any pending text
			const pendingTexts = tts.stopPlayback();

			// Apply new voice and speed
			(async () => {
				try {
					await tts.setVoice(vizVoice.voice);
					await tts.setSpeed(vizVoice.speed);

					// Re-queue pending text with new voice
					if (pendingTexts.length > 0) {
						console.log(
							"[Voice] Re-queuing",
							pendingTexts.length,
							"pending texts with new voice",
						);
						for (const text of pendingTexts) {
							tts.speak(text).catch(console.error);
						}
					}
				} catch (err) {
					console.error("[Voice] Failed to switch voice:", err);
				}
			})();
		}
	}, [settings.visualizer, visualizerVoices]);

	// Volume smoothing animation loop
	useEffect(() => {
		if (!isActive) {
			smoothInputRef.current = 0;
			smoothOutputRef.current = 0;
			setInputVolume(0);
			setOutputVolume(0);
			return;
		}

		const smoothingFactor = 0.15;

		const animate = () => {
			// Get raw volumes
			const rawInput = sttRef.current?.getInputVolume() ?? 0;
			const rawOutput = ttsRef.current?.getOutputVolume() ?? 0;

			// Apply exponential moving average
			smoothInputRef.current +=
				(rawInput - smoothInputRef.current) * smoothingFactor;
			smoothOutputRef.current +=
				(rawOutput - smoothOutputRef.current) * smoothingFactor;

			// Clamp near-zero values
			if (smoothInputRef.current < 0.001) smoothInputRef.current = 0;
			if (smoothOutputRef.current < 0.001) smoothOutputRef.current = 0;

			setInputVolume(smoothInputRef.current);
			setOutputVolume(smoothOutputRef.current);

			animationRef.current = requestAnimationFrame(animate);
		};

		animate();

		return () => {
			if (animationRef.current) {
				cancelAnimationFrame(animationRef.current);
				animationRef.current = null;
			}
		};
	}, [isActive]);

	// Handle final transcript
	const handleFinalTranscript = useCallback(
		async (text: string) => {
			if (!text.trim()) return;

			console.log("[Voice] Final transcript:", text);
			// Clear word array and UI state
			liveWordsRef.current = [];
			setLiveTranscript("");
			setVoiceState("processing");

			// Send transcript to chat
			onTranscript(text);

			// Note: The response handling and TTS will be triggered externally
			// when new assistant messages arrive
		},
		[onTranscript],
	);

	// Refs to access current state in callbacks (avoids stale closures)
	const settingsRef = useRef(settings);
	const voiceStateRef = useRef(voiceState);
	const isActiveRef = useRef(isActive);

	// Keep refs in sync
	useEffect(() => {
		settingsRef.current = settings;
	}, [settings]);
	useEffect(() => {
		voiceStateRef.current = voiceState;
	}, [voiceState]);
	useEffect(() => {
		isActiveRef.current = isActive;
	}, [isActive]);

	// Initialize services
	const initServices = useCallback(async () => {
		if (!config) {
			throw new Error("Voice mode not configured");
		}

		// Create STT service
		if (!sttRef.current) {
			sttRef.current = new STTService(
				voiceProxyWsUrl("stt"),
				settings.vadTimeoutMs,
			);
			sttRef.current.setCallbacks({
				onWord: (word) => {
					// Ignore if voice mode is not active or mic is muted
					if (!isActiveRef.current || settingsRef.current.micMuted) {
						return;
					}
					const tts = ttsRef.current;
					if (tts?.getIsPlaying()) {
						const outputLevel = tts.getOutputVolume();
						const inputLevel = sttRef.current?.getInputVolume() ?? 0;
						const startedAt = ttsStartedAtRef.current;
						const ageMs = startedAt ? Date.now() - startedAt : 0;
						const likelyEcho =
							outputLevel > 0.02 && inputLevel <= outputLevel * 1.2;
						if (likelyEcho && ageMs < 1500) {
							return;
						}
					}
					// O(1) array push instead of O(n) string concatenation
					liveWordsRef.current.push(word);
					setLiveTranscript(liveWordsRef.current.join(" "));

					// Check for interrupt-by-speaking while TTS is playing
					if (
						voiceStateRef.current === "speaking" &&
						settingsRef.current.interruptWordCount > 0
					) {
						interruptWordCountRef.current++;
						console.log(
							"[Voice] Word during TTS:",
							word,
							"count:",
							interruptWordCountRef.current,
						);

						// Reset backoff timer on each word
						if (interruptBackoffTimerRef.current) {
							window.clearTimeout(interruptBackoffTimerRef.current);
						}

						// Start backoff timer to reset word count after silence
						if (settingsRef.current.interruptBackoffMs > 0) {
							interruptBackoffTimerRef.current = window.setTimeout(() => {
								console.log("[Voice] Interrupt backoff - resetting word count");
								interruptWordCountRef.current = 0;
								interruptBackoffTimerRef.current = null;
							}, settingsRef.current.interruptBackoffMs);
						}

						if (
							interruptWordCountRef.current >=
							settingsRef.current.interruptWordCount
						) {
							console.log("[Voice] Interrupt threshold reached, stopping TTS");
							ttsRef.current?.stopPlayback();
							setVoiceState("listening");
							interruptWordCountRef.current = 0;
							// Clear backoff timer since we interrupted
							if (interruptBackoffTimerRef.current) {
								window.clearTimeout(interruptBackoffTimerRef.current);
								interruptBackoffTimerRef.current = null;
							}
						}
					}
				},
				onFinal: (text) => {
					// Ignore if voice mode is not active or mic is muted
					if (!isActiveRef.current || settingsRef.current.micMuted) {
						return;
					}
					handleFinalTranscript(text);
				},
				onVadProgress: (progress) => {
					// Ignore if voice mode is not active or mic is muted
					if (!isActiveRef.current || settingsRef.current.micMuted) {
						return;
					}
					setVadProgress(progress);
				},
				onError: (err) => {
					console.error("[Voice] STT error:", err);
					setError(err);
				},
				onConnectionChange: (connected) => {
					console.log("[Voice] STT connection:", connected);
				},
			});
		}

		// Create TTS service
		if (!ttsRef.current) {
			ttsRef.current = new TTSService(voiceProxyWsUrl("tts"));
			ttsRef.current.setCallbacks({
				onPlaying: () => {
					setVoiceState("speaking");
					// Reset interrupt word count when TTS starts
					interruptWordCountRef.current = 0;
					ttsStartedAtRef.current = Date.now();
				},
				onStopped: () => {
					// Return to listening if continuous mode and still active
					if (
						settingsRef.current.continuous &&
						isActiveRef.current &&
						!settingsRef.current.micMuted
					) {
						setVoiceState("listening");
						if (!sttRef.current?.getIsListening()) {
							sttRef.current?.startListening().catch(console.error);
						}
					} else {
						setVoiceState("idle");
					}
					interruptWordCountRef.current = 0;
					ttsStartedAtRef.current = null;
				},
				onVoicesLoaded: (voices, currentVoice) => {
					setAvailableVoices(voices);
				},
				onError: (err) => {
					console.error("[Voice] TTS error:", err);
					setError(err);
				},
			});
		}

		// Connect both services
		await Promise.all([sttRef.current.connect(), ttsRef.current.connect()]);

		// Apply settings
		ttsRef.current.setMuted(settings.muted);
		if (settings.voice) {
			await ttsRef.current.setVoice(settings.voice).catch(console.error);
		}
		await ttsRef.current.setSpeed(settings.speed).catch(console.error);

		setIsConnected(true);
	}, [config, settings, handleFinalTranscript]);

	// Start voice mode
	const start = useCallback(async () => {
		setError(null);

		try {
			await initServices();
			setIsActive(true);
			if (settingsRef.current.micMuted) {
				setVoiceState("idle");
				return;
			}
			setVoiceState("listening");
			await sttRef.current?.startListening();
		} catch (err) {
			const message =
				err instanceof Error ? err.message : "Failed to start voice mode";
			setError(message);
			console.error("[Voice] Start failed:", err);
			throw err;
		}
	}, [initServices]);

	// Stop voice mode
	const stop = useCallback(() => {
		setIsActive(false);
		setVoiceState("idle");
		liveWordsRef.current = [];
		setLiveTranscript("");
		setVadProgress(0);

		sttRef.current?.stopListening();
		ttsRef.current?.stopPlayback();
	}, []);

	// Interrupt TTS
	const interrupt = useCallback(() => {
		ttsRef.current?.stopPlayback();
		if (isActive && settings.continuous && !settings.micMuted) {
			setVoiceState("listening");
			sttRef.current?.startListening().catch(console.error);
		}
	}, [isActive, settings.continuous, settings.micMuted]);

	// Speak text
	const speak = useCallback(
		async (text: string) => {
			// Don't speak if voice mode is not active
			// Check both the state and ref to handle the React effect timing gap
			if (!isActive || !isActiveRef.current) {
				console.log("[Voice] Ignoring speak - voice mode not active");
				return;
			}

			if (!ttsRef.current?.isConnected()) {
				console.warn("[Voice] TTS not connected");
				return;
			}

			// Keep listening for interrupt-by-speaking (if enabled)
			// Only stop listening if interrupt is disabled
			// Note: we use settingsRef here to avoid stale closure issues
			if (settingsRef.current.micMuted) {
				sttRef.current?.stopListening();
			} else if (settingsRef.current.interruptWordCount <= 0) {
				sttRef.current?.stopListening();
			} else {
				// Make sure we're listening for potential interrupt
				if (!sttRef.current?.getIsListening()) {
					sttRef.current?.startListening().catch(console.error);
				}
			}

			setVoiceState("speaking");
			interruptWordCountRef.current = 0;
			await ttsRef.current.speak(text);
		},
		[isActive],
	);

	// Settings setters
	const setVisualizer = useCallback((type: VisualizerType) => {
		setSettings((prev) => ({ ...prev, visualizer: type }));
	}, []);

	const setMuted = useCallback((muted: boolean) => {
		setSettings((prev) => ({ ...prev, muted }));
		ttsRef.current?.setMuted(muted);
	}, []);

	const setMicMuted = useCallback((muted: boolean) => {
		setSettings((prev) => ({ ...prev, micMuted: muted }));
		if (muted) {
			sttRef.current?.stopListening();
			setLiveTranscript("");
			setVadProgress(0);
			if (
				voiceStateRef.current === "listening" ||
				voiceStateRef.current === "processing"
			) {
				setVoiceState("idle");
			}
			return;
		}

		if (isActiveRef.current) {
			setVoiceState("listening");
			sttRef.current?.startListening().catch(console.error);
		}
	}, []);

	const setContinuous = useCallback((continuous: boolean) => {
		setSettings((prev) => ({ ...prev, continuous }));
	}, []);

	const setVoice = useCallback(
		async (voice: string) => {
			setSettings((prev) => ({ ...prev, voice }));
			// Also save to per-visualizer settings
			setVisualizerVoices((prev) => ({
				...prev,
				[settings.visualizer]: { ...prev[settings.visualizer], voice },
			}));
			if (ttsRef.current?.isConnected()) {
				await ttsRef.current.setVoice(voice).catch(console.error);
			}
		},
		[settings.visualizer],
	);

	const setSpeed = useCallback(
		async (speed: number) => {
			setSettings((prev) => ({ ...prev, speed }));
			// Also save to per-visualizer settings
			setVisualizerVoices((prev) => ({
				...prev,
				[settings.visualizer]: { ...prev[settings.visualizer], speed },
			}));
			if (ttsRef.current?.isConnected()) {
				await ttsRef.current.setSpeed(speed).catch(console.error);
			}
		},
		[settings.visualizer],
	);

	const setVadTimeout = useCallback((ms: number) => {
		setSettings((prev) => ({ ...prev, vadTimeoutMs: ms }));
		sttRef.current?.setVadTimeout(ms);
	}, []);

	const setInterruptWordCount = useCallback((count: number) => {
		setSettings((prev) => ({
			...prev,
			interruptWordCount: Math.max(0, count),
		}));
	}, []);

	// Streaming TTS methods
	const streamStart = useCallback(async (): Promise<string> => {
		if (!isActive || !isActiveRef.current) {
			throw new Error("Voice mode not active");
		}
		if (!ttsRef.current?.isConnected()) {
			throw new Error("TTS not connected");
		}

		// Set up listening for interrupts if enabled
		if (
			!settingsRef.current.micMuted &&
			settingsRef.current.interruptWordCount > 0
		) {
			if (!sttRef.current?.getIsListening()) {
				sttRef.current?.startListening().catch(console.error);
			}
		}

		setVoiceState("speaking");
		interruptWordCountRef.current = 0;
		return ttsRef.current.streamStart();
	}, [isActive]);

	const streamAppend = useCallback(
		(text: string): void => {
			if (!isActive || !isActiveRef.current) return;
			ttsRef.current?.streamAppend(text);
		},
		[isActive],
	);

	const streamEnd = useCallback((): void => {
		ttsRef.current?.streamEnd();
	}, []);

	const streamCancel = useCallback((): void => {
		ttsRef.current?.streamCancel();
	}, []);

	// Cleanup on unmount
	useEffect(() => {
		return () => {
			sttRef.current?.disconnect();
			ttsRef.current?.disconnect();
			if (animationRef.current) {
				cancelAnimationFrame(animationRef.current);
			}
		};
	}, []);

	return {
		// State
		isActive,
		voiceState,
		liveTranscript,
		vadProgress,
		inputVolume,
		outputVolume,
		isConnected,
		error,

		// Settings
		settings,
		setVisualizer,
		setMuted,
		setMicMuted,
		setContinuous,
		setVoice,
		setSpeed,
		setVadTimeout,
		setInterruptWordCount,
		availableVoices,

		// Actions
		start,
		stop,
		interrupt,
		speak,

		// Streaming TTS
		streamStart,
		streamAppend,
		streamEnd,
		streamCancel,
		isStreaming: ttsRef.current?.isStreaming() ?? false,
	};
}
