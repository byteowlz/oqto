/**
 * Dictation hook for speech-to-text input.
 *
 * Unlike voice mode, dictation only does STT and appends to a text input.
 * It doesn't trigger TTS responses - it's just for typing by speaking.
 */

import { STTService } from "@/lib/voice";
import type { VoiceConfig } from "@/lib/voice/types";
import { useCallback, useEffect, useRef, useState } from "react";

export interface UseDictationOptions {
	/** Voice configuration from backend (for STT URL) */
	config: VoiceConfig | null;
	/** Callback when text is transcribed - appends to input */
	onTranscript: (text: string) => void;
	/** VAD timeout in ms (default: use config or 2000ms) */
	vadTimeoutMs?: number;
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
	/** Start dictation */
	start: () => Promise<void>;
	/** Stop dictation */
	stop: () => void;
}

/**
 * Hook for dictation mode - speech to text input.
 */
export function useDictation(options: UseDictationOptions): UseDictationReturn {
	const { config, onTranscript, vadTimeoutMs } = options;

	const sttRef = useRef<STTService | null>(null);
	const animationRef = useRef<number | null>(null);
	const smoothVolumeRef = useRef(0);
	// Track the live transcript in a ref to avoid race conditions between VAD final and manual stop
	const liveTranscriptRef = useRef("");

	const [isActive, setIsActive] = useState(false);
	const [liveTranscript, setLiveTranscript] = useState("");
	const [vadProgress, setVadProgress] = useState(0);
	const [inputVolume, setInputVolume] = useState(0);
	const [isConnected, setIsConnected] = useState(false);
	const [error, setError] = useState<string | null>(null);

	// Keep refs in sync for callbacks
	const onTranscriptRef = useRef(onTranscript);
	useEffect(() => {
		onTranscriptRef.current = onTranscript;
	}, [onTranscript]);

	// Volume smoothing animation loop
	useEffect(() => {
		if (!isActive) {
			smoothVolumeRef.current = 0;
			setInputVolume(0);
			return;
		}

		const smoothingFactor = 0.15;

		const animate = () => {
			const rawVolume = sttRef.current?.getInputVolume() ?? 0;
			smoothVolumeRef.current +=
				(rawVolume - smoothVolumeRef.current) * smoothingFactor;
			if (smoothVolumeRef.current < 0.001) smoothVolumeRef.current = 0;
			setInputVolume(smoothVolumeRef.current);
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

	// Handle final transcript - append to input
	const handleFinalTranscript = useCallback((text: string) => {
		if (!text.trim()) return;
		console.log("[Dictation] Final transcript:", text);
		// Clear both state and ref to prevent double-submission on stop
		liveTranscriptRef.current = "";
		setLiveTranscript("");
		onTranscriptRef.current(text);
	}, []);

	// Initialize STT service
	const initService = useCallback(async () => {
		if (!config) {
			throw new Error("Dictation not configured - voice config missing");
		}

		if (!sttRef.current) {
			const timeout = vadTimeoutMs ?? config.vad_timeout_ms ?? 2000;
			sttRef.current = new STTService(config.stt_url, timeout);
			sttRef.current.setCallbacks({
				onWord: (word) => {
					setLiveTranscript((prev) => {
						const newTranscript = `${prev ? `${prev} ` : ""}${word}`;
						liveTranscriptRef.current = newTranscript;
						return newTranscript;
					});
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

	// Stop dictation - flush any pending transcript first
	const stop = useCallback(() => {
		console.log("[Dictation] Stopping");
		// Use ref to check for pending transcript - avoids race condition with handleFinalTranscript
		// which may have already cleared the transcript via VAD timeout
		const pendingTranscript = liveTranscriptRef.current.trim();
		if (pendingTranscript) {
			console.log(
				"[Dictation] Flushing pending transcript:",
				pendingTranscript,
			);
			onTranscriptRef.current(pendingTranscript);
		}
		// Clear both ref and state
		liveTranscriptRef.current = "";
		setLiveTranscript("");
		setIsActive(false);
		setVadProgress(0);
		sttRef.current?.stopListening();
	}, []);

	// Cleanup on unmount
	useEffect(() => {
		return () => {
			sttRef.current?.disconnect();
			if (animationRef.current) {
				cancelAnimationFrame(animationRef.current);
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
		start,
		stop,
	};
}
