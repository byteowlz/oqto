/**
 * Hook for text-to-speech functionality using kokorox WebSocket.
 * Provides a simple interface for reading text aloud with configurable voice/speed.
 */

import { voiceProxyWsUrl } from "@/lib/control-plane-client";
import { TTSService } from "@/lib/voice/tts-service";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

export type TTSState = "idle" | "connecting" | "speaking" | "error";

const TTS_SETTINGS_KEY = "octo-tts-read-aloud-settings";

export interface TTSSettings {
	voice: string;
	speed: number;
}

const DEFAULT_TTS_SETTINGS: TTSSettings = {
	voice: "af_heart",
	speed: 1.3,
};

/** Load TTS settings from localStorage */
function loadTTSSettings(): TTSSettings {
	if (typeof window === "undefined") return DEFAULT_TTS_SETTINGS;
	try {
		const stored = localStorage.getItem(TTS_SETTINGS_KEY);
		if (stored) {
			return { ...DEFAULT_TTS_SETTINGS, ...JSON.parse(stored) };
		}
	} catch (e) {
		console.error("[TTS] Failed to load settings:", e);
	}
	return DEFAULT_TTS_SETTINGS;
}

/** Save TTS settings to localStorage */
function saveTTSSettings(settings: TTSSettings) {
	if (typeof window === "undefined") return;
	try {
		localStorage.setItem(TTS_SETTINGS_KEY, JSON.stringify(settings));
	} catch (e) {
		console.error("[TTS] Failed to save settings:", e);
	}
}

/** Split text into paragraphs */
function splitIntoParagraphs(text: string): string[] {
	return text
		.split(/\n\n+/)
		.map((p) => p.trim())
		.filter((p) => p.length > 0);
}

export interface UseTTSResult {
	/** Current state of the TTS service */
	state: TTSState;
	/** Whether currently speaking */
	isSpeaking: boolean;
	/** Whether the service is connected */
	isConnected: boolean;
	/** Speak the given text */
	speak: (text: string) => Promise<void>;
	/** Stop current playback */
	stop: () => void;
	/** Error message if state is "error" */
	error: string | null;
	/** Current settings */
	settings: TTSSettings;
	/** Available voices (populated after connection) */
	availableVoices: string[];
	/** Set the voice */
	setVoice: (voice: string) => Promise<void>;
	/** Set the speed */
	setSpeed: (speed: number) => Promise<void>;
}

export interface UseTTSWithParagraphsResult extends UseTTSResult {
	/** Start reading from beginning or current paragraph */
	play: () => void;
	/** Go to previous paragraph */
	previousParagraph: () => void;
	/** Go to next paragraph */
	nextParagraph: () => void;
	/** Current paragraph index */
	currentParagraph: number;
	/** Total number of paragraphs */
	totalParagraphs: number;
	/** Whether there's a previous paragraph */
	hasPrevious: boolean;
	/** Whether there's a next paragraph */
	hasNext: boolean;
}

/**
 * Hook for text-to-speech using kokorox WebSocket.
 * Lazily connects on first speak() call.
 */
export function useTTS(): UseTTSResult {
	const [state, setState] = useState<TTSState>("idle");
	const [isConnected, setIsConnected] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [settings, setSettings] = useState<TTSSettings>(loadTTSSettings);
	const [availableVoices, setAvailableVoices] = useState<string[]>([]);

	const ttsRef = useRef<TTSService | null>(null);
	const connectingRef = useRef(false);
	const isConnectedRef = useRef(false);
	const connectionPromiseRef = useRef<Promise<TTSService> | null>(null);

	// Keep ref in sync with state
	useEffect(() => {
		isConnectedRef.current = isConnected;
	}, [isConnected]);

	// Cleanup on unmount
	useEffect(() => {
		return () => {
			if (ttsRef.current) {
				ttsRef.current.stopPlayback();
				ttsRef.current.disconnect();
				ttsRef.current = null;
			}
		};
	}, []);

	const ensureConnected = useCallback(async (): Promise<TTSService> => {
		// Already have a connected service
		if (ttsRef.current?.isConnected()) {
			return ttsRef.current;
		}

		// Already connecting - return the existing promise
		if (connectingRef.current && connectionPromiseRef.current) {
			return connectionPromiseRef.current;
		}

		// Create new service and connect
		connectingRef.current = true;
		setState("connecting");
		setError(null);

		const connectPromise = (async (): Promise<TTSService> => {
			try {
				const wsUrl = voiceProxyWsUrl("tts");
				const tts = new TTSService(wsUrl);

				tts.setCallbacks({
					onConnectionChange: (connected) => {
						setIsConnected(connected);
						isConnectedRef.current = connected;
						if (!connected) {
							setState("idle");
						}
					},
					onPlaying: () => {
						setState("speaking");
					},
					onStopped: () => {
						setState("idle");
					},
					onError: (err) => {
						setError(err);
						setState("error");
					},
					onVoicesLoaded: (voices, currentVoice) => {
						setAvailableVoices(voices);
						// If stored voice is not available, update to current
						const storedSettings = loadTTSSettings();
						if (voices.length > 0 && !voices.includes(storedSettings.voice)) {
							const newSettings = { ...storedSettings, voice: currentVoice };
							setSettings(newSettings);
							saveTTSSettings(newSettings);
						}
					},
				});

				await tts.connect();

				// Apply stored settings
				const storedSettings = loadTTSSettings();
				try {
					await tts.setVoice(storedSettings.voice);
					await tts.setSpeed(storedSettings.speed);
				} catch (e) {
					console.warn("[TTS] Failed to apply stored settings:", e);
				}

				ttsRef.current = tts;
				connectingRef.current = false;
				connectionPromiseRef.current = null;
				setState("idle");
				return tts;
			} catch (err) {
				connectingRef.current = false;
				connectionPromiseRef.current = null;
				const message =
					err instanceof Error ? err.message : "Failed to connect";
				setError(message);
				setState("error");
				throw err;
			}
		})();

		connectionPromiseRef.current = connectPromise;
		return connectPromise;
	}, []);

	const speak = useCallback(
		async (text: string) => {
			if (!text.trim()) return;

			try {
				const tts = await ensureConnected();
				// Don't set speaking state here - the onPlaying callback will handle it
				await tts.speak(text);
				// Only set idle if we're still in speaking state (not already stopped)
				setState((prev) => (prev === "speaking" ? "idle" : prev));
			} catch (err) {
				// Ignore "Playback stopped" errors - these are expected when user cancels
				const message = err instanceof Error ? err.message : "TTS failed";
				if (message === "Playback stopped") {
					return;
				}
				setError(message);
				setState("error");
			}
		},
		[ensureConnected],
	);

	const stop = useCallback(() => {
		if (ttsRef.current) {
			ttsRef.current.stopPlayback();
			setState("idle");
		}
	}, []);

	const setVoice = useCallback(
		async (voice: string) => {
			const newSettings = { ...settings, voice };
			setSettings(newSettings);
			saveTTSSettings(newSettings);

			if (ttsRef.current && isConnected) {
				try {
					await ttsRef.current.setVoice(voice);
				} catch (e) {
					console.error("[TTS] Failed to set voice:", e);
				}
			}
		},
		[settings, isConnected],
	);

	const setSpeed = useCallback(
		async (speed: number) => {
			const clampedSpeed = Math.max(0.5, Math.min(2.0, speed));
			const newSettings = { ...settings, speed: clampedSpeed };
			setSettings(newSettings);
			saveTTSSettings(newSettings);

			if (ttsRef.current && isConnected) {
				try {
					await ttsRef.current.setSpeed(clampedSpeed);
				} catch (e) {
					console.error("[TTS] Failed to set speed:", e);
				}
			}
		},
		[settings, isConnected],
	);

	return {
		state,
		isSpeaking: state === "speaking",
		isConnected,
		speak,
		stop,
		error,
		settings,
		availableVoices,
		setVoice,
		setSpeed,
	};
}

/**
 * Hook for text-to-speech with paragraph navigation.
 * Splits text into paragraphs and allows jumping between them.
 */
export function useTTSWithParagraphs(text: string): UseTTSWithParagraphsResult {
	const tts = useTTS();
	const [currentIndex, setCurrentIndex] = useState(0);
	const isPlayingRef = useRef(false);
	// Unique session ID to prevent race conditions when rapidly switching paragraphs
	const sessionIdRef = useRef(0);

	const paragraphs = useMemo(() => splitIntoParagraphs(text), [text]);
	const lastTextRef = useRef(text);

	// Reset to first paragraph when text changes
	useEffect(() => {
		if (lastTextRef.current !== text) {
			lastTextRef.current = text;
			setCurrentIndex(0);
			sessionIdRef.current++; // Invalidate any running sessions
			tts.stop();
		}
	}, [text, tts]);

	const speakParagraph = useCallback(
		async (index: number, sessionId: number, continueToNext = true) => {
			if (index < 0 || index >= paragraphs.length) return;

			// Check if this session is still valid
			if (sessionId !== sessionIdRef.current) {
				console.log(
					"[TTS] Session invalidated, aborting speakParagraph",
					sessionId,
					"vs",
					sessionIdRef.current,
				);
				return; // Session was invalidated, abort
			}

			isPlayingRef.current = true;
			setCurrentIndex(index);

			try {
				// Check again before starting async operation
				if (sessionId !== sessionIdRef.current) {
					return;
				}

				await tts.speak(paragraphs[index]);

				// Check session validity again after async operation
				if (sessionId !== sessionIdRef.current) {
					console.log(
						"[TTS] Session invalidated after speak",
						sessionId,
						"vs",
						sessionIdRef.current,
					);
					return; // Session was invalidated during playback
				}

				// If continueToNext, proceed to next paragraph
				if (continueToNext && index < paragraphs.length - 1) {
					// Small delay between paragraphs
					await new Promise((resolve) => setTimeout(resolve, 300));

					// Final check before recursing
					if (sessionId === sessionIdRef.current) {
						await speakParagraph(index + 1, sessionId, true);
					}
				}
			} catch (err) {
				// Ignore errors from stopped playback
				if (sessionId !== sessionIdRef.current) {
					return;
				}
				console.error("[TTS] speakParagraph error:", err);
			} finally {
				// Only update playing state if this is still the active session
				if (sessionId === sessionIdRef.current) {
					isPlayingRef.current = false;
				}
			}
		},
		[paragraphs, tts],
	);

	const play = useCallback(() => {
		if (tts.isSpeaking) {
			sessionIdRef.current++; // Invalidate current session
			tts.stop();
		} else {
			const newSession = ++sessionIdRef.current;
			speakParagraph(currentIndex, newSession, true);
		}
	}, [tts, currentIndex, speakParagraph]);

	const stop = useCallback(() => {
		sessionIdRef.current++; // Invalidate current session
		tts.stop();
	}, [tts]);

	const previousParagraph = useCallback(() => {
		if (currentIndex > 0) {
			// Increment session first, then stop, then start new playback
			const newSession = ++sessionIdRef.current;
			tts.stop();
			const newIndex = currentIndex - 1;
			setCurrentIndex(newIndex);
			// Use requestAnimationFrame to ensure state updates are flushed
			requestAnimationFrame(() => {
				// Double-check session is still valid
				if (newSession === sessionIdRef.current) {
					speakParagraph(newIndex, newSession, true);
				}
			});
		}
	}, [currentIndex, tts, speakParagraph]);

	const nextParagraph = useCallback(() => {
		if (currentIndex < paragraphs.length - 1) {
			// Increment session first, then stop, then start new playback
			const newSession = ++sessionIdRef.current;
			tts.stop();
			const newIndex = currentIndex + 1;
			setCurrentIndex(newIndex);
			// Use requestAnimationFrame to ensure state updates are flushed
			requestAnimationFrame(() => {
				// Double-check session is still valid
				if (newSession === sessionIdRef.current) {
					speakParagraph(newIndex, newSession, true);
				}
			});
		}
	}, [currentIndex, paragraphs.length, tts, speakParagraph]);

	return {
		...tts,
		stop,
		play,
		previousParagraph,
		nextParagraph,
		currentParagraph: currentIndex,
		totalParagraphs: paragraphs.length,
		hasPrevious: currentIndex > 0,
		hasNext: currentIndex < paragraphs.length - 1,
	};
}
