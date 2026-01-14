/**
 * Dictation hook for speech-to-text input.
 *
 * Unlike voice mode, dictation only does STT and appends to a text input.
 * It doesn't trigger TTS responses - it's just for typing by speaking.
 *
 * Performance: Uses word array instead of string concatenation to avoid O(n^2) growth.
 */

import { voiceProxyWsUrl } from "@/lib/control-plane-client";
import { STTService } from "@/lib/voice";
import type { VoiceConfig } from "@/lib/voice/types";
import { useCallback, useEffect, useRef, useState } from "react";

export interface UseDictationOptions {
	/** Voice configuration from backend (for STT URL) */
	config: VoiceConfig | null;
	/** Callback when text is transcribed (usually append to input) */
	onTranscript: (text: string) => void;
	/** VAD timeout in ms (default: use config or 2000ms) */
	vadTimeoutMs?: number;
	/** If true, call onAutoSend after each VAD final */
	autoSendOnFinal?: boolean;
	/** Optional delay before auto-sending (ms) */
	autoSendDelayMs?: number;
	/** Callback to trigger sending the current input */
	onAutoSend?: () => void;
}

export interface UseDictationReturn {
	/** Whether dictation is currently active */
	isActive: boolean;
	/** Current live transcript being accumulated */
	liveTranscript: string;
	/** VAD progress (0-1) for timeout visualization */
	vadProgress: number;
	/** Input volume (0-1) for visualization */
	inputVolume: number;
	/** Whether connected to STT service */
	isConnected: boolean;
	/** Error message if any */
	error: string | null;
	/** Whether auto-send on VAD final is enabled */
	autoSendEnabled: boolean;
	/** Toggle auto-send on/off */
	setAutoSendEnabled: (enabled: boolean) => void;
	/** Start dictation */
	start: () => Promise<void>;
	/** Stop dictation (flushes pending transcript, may trigger auto-send) */
	stop: () => void;
	/** Cancel dictation (flushes pending transcript, does NOT auto-send) */
	cancel: () => void;
}

/**
 * Hook for dictation mode - speech to text input.
 */
export function useDictation(options: UseDictationOptions): UseDictationReturn {
	const {
		config,
		onTranscript,
		vadTimeoutMs,
		autoSendOnFinal = false,
		autoSendDelayMs = 0,
		onAutoSend,
	} = options;

	const sttRef = useRef<STTService | null>(null);
	const smoothVolumeRef = useRef(0);

	// Use word array instead of string concatenation (O(1) push vs O(n) concat)
	const liveWordsRef = useRef<string[]>([]);

	const [isActive, setIsActive] = useState(false);
	const [liveTranscript, setLiveTranscript] = useState("");
	const [vadProgress, setVadProgress] = useState(0);
	const [inputVolume, setInputVolume] = useState(0);
	const [isConnected, setIsConnected] = useState(false);
	const [error, setError] = useState<string | null>(null);
	// Auto-send state - initialized from options, can be toggled by user
	const [autoSendEnabled, setAutoSendEnabled] = useState(autoSendOnFinal);

	// Keep refs in sync for callbacks
	const onTranscriptRef = useRef(onTranscript);
	const onAutoSendRef = useRef(onAutoSend);
	const autoSendEnabledRef = useRef(autoSendEnabled);
	const autoSendDelayRef = useRef(autoSendDelayMs);

	useEffect(() => {
		onTranscriptRef.current = onTranscript;
	}, [onTranscript]);
	useEffect(() => {
		onAutoSendRef.current = onAutoSend;
	}, [onAutoSend]);
	useEffect(() => {
		autoSendEnabledRef.current = autoSendEnabled;
	}, [autoSendEnabled]);
	useEffect(() => {
		autoSendDelayRef.current = autoSendDelayMs;
	}, [autoSendDelayMs]);

	// Throttle transcript updates to reduce re-renders (update at most every 100ms)
	const lastTranscriptUpdateRef = useRef(0);
	const pendingTranscriptUpdateRef = useRef<number | null>(null);
	// Track auto-send timeout so it can be canceled
	const autoSendTimeoutRef = useRef<number | null>(null);

	// Volume smoothing loop - throttled to ~15fps to reduce re-renders
	useEffect(() => {
		if (!isActive) {
			smoothVolumeRef.current = 0;
			setInputVolume(0);
			return;
		}

		const smoothingFactor = 0.3; // Higher factor since we update less frequently

		const intervalId = setInterval(() => {
			const rawVolume = sttRef.current?.getInputVolume() ?? 0;
			smoothVolumeRef.current +=
				(rawVolume - smoothVolumeRef.current) * smoothingFactor;
			if (smoothVolumeRef.current < 0.001) smoothVolumeRef.current = 0;
			setInputVolume(smoothVolumeRef.current);
		}, 66); // ~15fps instead of 60fps

		return () => {
			clearInterval(intervalId);
		};
	}, [isActive]);

	// Handle final transcript - append to input
	const handleFinalTranscript = useCallback((text: string) => {
		if (!text.trim()) return;
		console.log("[Dictation] Final transcript:", text);
		// Clear both state and ref to prevent double-submission on stop
		liveWordsRef.current = [];
		setLiveTranscript("");

		onTranscriptRef.current(text);

		if (autoSendEnabledRef.current && onAutoSendRef.current) {
			// Clear any existing timeout before setting a new one
			if (autoSendTimeoutRef.current !== null) {
				window.clearTimeout(autoSendTimeoutRef.current);
			}
			const delay = autoSendDelayRef.current;
			autoSendTimeoutRef.current = window.setTimeout(() => {
				autoSendTimeoutRef.current = null;
				onAutoSendRef.current?.();
			}, delay);
		}
	}, []);

	// Initialize STT service
	const initService = useCallback(async () => {
		if (!config) {
			throw new Error("Dictation not configured - voice config missing");
		}

		if (!sttRef.current) {
			const timeout = vadTimeoutMs ?? config.vad_timeout_ms ?? 2000;
			// Use voiceProxyWsUrl to get the full URL with auth token
			sttRef.current = new STTService(voiceProxyWsUrl("stt"), timeout);
			sttRef.current.setCallbacks({
				onWord: (word) => {
					// O(1) array push instead of O(n) string concatenation
					liveWordsRef.current.push(word);

					// Throttle React state updates to max 10/sec to reduce re-renders
					const now = Date.now();
					if (now - lastTranscriptUpdateRef.current >= 100) {
						lastTranscriptUpdateRef.current = now;
						// Only join when updating UI (single allocation)
						setLiveTranscript(liveWordsRef.current.join(" "));
					} else if (!pendingTranscriptUpdateRef.current) {
						// Schedule update for end of throttle window
						pendingTranscriptUpdateRef.current = window.setTimeout(
							() => {
								pendingTranscriptUpdateRef.current = null;
								lastTranscriptUpdateRef.current = Date.now();
								setLiveTranscript(liveWordsRef.current.join(" "));
							},
							100 - (now - lastTranscriptUpdateRef.current),
						);
					}
				},
				onFinal: handleFinalTranscript,
				onVadProgress: setVadProgress,
				onError: (err) => {
					console.error("[Dictation] STT error:", err);
					setError(err);
				},
				onConnectionChange: (connected) => {
					console.log("[Dictation] STT connection:", connected);
					setIsConnected(connected);
				},
			});
		}

		await sttRef.current.connect();
		setIsConnected(true);
	}, [config, vadTimeoutMs, handleFinalTranscript]);

	// Start dictation
	const start = useCallback(async () => {
		setError(null);

		try {
			await initService();
			setIsActive(true);
			await sttRef.current?.startListening();
			console.log("[Dictation] Started");
		} catch (err) {
			const message =
				err instanceof Error ? err.message : "Failed to start dictation";
			setError(message);
			console.error("[Dictation] Start failed:", err);
			throw err;
		}
	}, [initService]);

	// Stop dictation - flush any pending transcript first (may trigger auto-send)
	const stop = useCallback(() => {
		console.log("[Dictation] Stopping");
		// Use ref to check for pending transcript - avoids race condition with handleFinalTranscript
		// which may have already cleared the transcript via VAD timeout
		if (liveWordsRef.current.length > 0) {
			const pendingTranscript = liveWordsRef.current.join(" ").trim();
			if (pendingTranscript) {
				console.log(
					"[Dictation] Flushing pending transcript:",
					pendingTranscript,
				);
				onTranscriptRef.current(pendingTranscript);
			}
		}
		// Clear both ref and state
		liveWordsRef.current = [];
		setLiveTranscript("");
		setIsActive(false);
		setVadProgress(0);
		sttRef.current?.stopListening();
	}, []);

	// Cancel dictation - flush pending transcript but do NOT auto-send
	const cancel = useCallback(() => {
		console.log("[Dictation] Canceling (no auto-send)");
		// Cancel any pending auto-send timeout
		if (autoSendTimeoutRef.current !== null) {
			window.clearTimeout(autoSendTimeoutRef.current);
			autoSendTimeoutRef.current = null;
		}
		// Flush pending transcript without triggering auto-send
		if (liveWordsRef.current.length > 0) {
			const pendingTranscript = liveWordsRef.current.join(" ").trim();
			if (pendingTranscript) {
				console.log(
					"[Dictation] Flushing pending transcript (no auto-send):",
					pendingTranscript,
				);
				onTranscriptRef.current(pendingTranscript);
			}
		}
		// Clear both ref and state
		liveWordsRef.current = [];
		setLiveTranscript("");
		setIsActive(false);
		setVadProgress(0);
		sttRef.current?.stopListening();
	}, []);

	// Cleanup on unmount
	useEffect(() => {
		return () => {
			sttRef.current?.disconnect();
			// Clear any pending transcript update timeout
			if (pendingTranscriptUpdateRef.current) {
				clearTimeout(pendingTranscriptUpdateRef.current);
			}
			// Clear any pending auto-send timeout
			if (autoSendTimeoutRef.current !== null) {
				clearTimeout(autoSendTimeoutRef.current);
			}
		};
	}, []);

	return {
		isActive,
		liveTranscript,
		vadProgress,
		inputVolume,
		isConnected,
		error,
		autoSendEnabled,
		setAutoSendEnabled,
		start,
		stop,
		cancel,
	};
}
