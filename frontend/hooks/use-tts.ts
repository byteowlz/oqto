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
		if (ttsRef.current && isConnected) {
			return ttsRef.current;
		}

		// Already connecting
		if (connectingRef.current && ttsRef.current) {
			// Wait for connection
			return new Promise((resolve, reject) => {
				const checkInterval = setInterval(() => {
					if (isConnected && ttsRef.current) {
						clearInterval(checkInterval);
						resolve(ttsRef.current);
					}
				}, 100);
				// Timeout after 10s
				setTimeout(() => {
					clearInterval(checkInterval);
					reject(new Error("Connection timeout"));
				}, 10000);
			});
		}

		// Create new service and connect
		connectingRef.current = true;
		setState("connecting");
		setError(null);

		try {
			const wsUrl = voiceProxyWsUrl("tts");
			const tts = new TTSService(wsUrl);

			tts.setCallbacks({
				onConnectionChange: (connected) => {
					setIsConnected(connected);
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
					if (voices.length > 0 && !voices.includes(settings.voice)) {
						const newSettings = { ...settings, voice: currentVoice };
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
			setState("idle");
			return tts;
		} catch (err) {
			connectingRef.current = false;
			const message = err instanceof Error ? err.message : "Failed to connect";
			setError(message);
			setState("error");
			throw err;
		}
	}, [isConnected, settings]);

	const speak = useCallback(
		async (text: string) => {
			if (!text.trim()) return;

			try {
				const tts = await ensureConnected();
				setState("speaking");
				await tts.speak(text);
				setState("idle");
			} catch (err) {
				const message = err instanceof Error ? err.message : "TTS failed";
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
	const abortRef = useRef(false);
	// Unique session ID to prevent race conditions when rapidly switching paragraphs
	const sessionIdRef = useRef(0);

	const paragraphs = useMemo(() => splitIntoParagraphs(text), [text]);
	const lastTextRef = useRef(text);

	// Reset to first paragraph when text changes
	useEffect(() => {
		if (lastTextRef.current !== text) {
			lastTextRef.current = text;
			setCurrentIndex(0);
			abortRef.current = true;
			sessionIdRef.current++; // Invalidate any running sessions
			tts.stop();
		}
	}, [text, tts]);

	const speakParagraph = useCallback(
		async (index: number, continueToNext = true, sessionId?: number) => {
			if (index < 0 || index >= paragraphs.length) return;

			// Use provided sessionId or create new one
			const currentSession = sessionId ?? ++sessionIdRef.current;

			// Check if this session is still valid
			if (currentSession !== sessionIdRef.current) {
				return; // Session was invalidated, abort
			}

			abortRef.current = false;
			isPlayingRef.current = true;
			setCurrentIndex(index);

			try {
				await tts.speak(paragraphs[index]);

				// Check session validity again after async operation
				if (currentSession !== sessionIdRef.current) {
					return; // Session was invalidated during playback
				}

				// If not aborted and continueToNext, proceed to next paragraph
				if (
					!abortRef.current &&
					continueToNext &&
					index < paragraphs.length - 1
				) {
					// Small delay between paragraphs
					await new Promise((resolve) => setTimeout(resolve, 300));

					// Final check before recursing
					if (!abortRef.current && currentSession === sessionIdRef.current) {
						await speakParagraph(index + 1, true, currentSession);
					}
				}
			} catch {
				// Ignore errors from stopped playback
				if (currentSession !== sessionIdRef.current) {
					return;
				}
			} finally {
				// Only update playing state if this is still the active session
				if (currentSession === sessionIdRef.current) {
					if (!abortRef.current || index >= paragraphs.length - 1) {
						isPlayingRef.current = false;
					}
				}
			}
		},
		[paragraphs, tts],
	);

	const play = useCallback(() => {
		if (tts.isSpeaking) {
			abortRef.current = true;
			sessionIdRef.current++; // Invalidate current session
			tts.stop();
		} else {
			speakParagraph(currentIndex, true);
		}
	}, [tts, currentIndex, speakParagraph]);

	const stop = useCallback(() => {
		abortRef.current = true;
		sessionIdRef.current++; // Invalidate current session
		tts.stop();
	}, [tts]);

	const previousParagraph = useCallback(() => {
		if (currentIndex > 0) {
			abortRef.current = true;
			sessionIdRef.current++; // Invalidate current session
			tts.stop();
			const newIndex = currentIndex - 1;
			setCurrentIndex(newIndex);
			// Start playing from the new paragraph with new session
			setTimeout(() => speakParagraph(newIndex, true), 50);
		}
	}, [currentIndex, tts, speakParagraph]);

	const nextParagraph = useCallback(() => {
		if (currentIndex < paragraphs.length - 1) {
			abortRef.current = true;
			sessionIdRef.current++; // Invalidate current session
			tts.stop();
			const newIndex = currentIndex + 1;
			setCurrentIndex(newIndex);
			// Start playing from the new paragraph with new session
			setTimeout(() => speakParagraph(newIndex, true), 50);
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
