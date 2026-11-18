import { useMountEffect } from "@/hooks/use-mount-effect";
import type { TTSDownloadProgress } from "@/lib/voice/phonon-model";
import {
	type ReadAloudService,
	createReadAloudService,
} from "@/lib/voice/read-aloud-service";
import {
	createParagraphPlayer,
	splitIntoParagraphs,
} from "@/lib/voice/tts-queue";
import {
	DEFAULT_TTS_SETTINGS,
	type TTSProvider,
	type TTSSettings,
	loadTTSSettings,
	saveTTSSettings,
	selectTTSProvider,
	subscribeTTSSettings,
	updatePhononTTSSettings,
} from "@/lib/voice/tts-settings";
import {
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";

export {
	DEFAULT_TTS_SETTINGS,
	TTS_SETTINGS_KEY,
	loadTTSSettings,
	saveTTSSettings,
	saveReadAloudSettings,
	selectTTSProvider,
	updatePhononTTSSettings,
	useTTSSettings,
	type TTSSettings,
	type TTSProvider,
} from "@/lib/voice/tts-settings";

export type TTSState = "idle" | "connecting" | "speaking" | "error";
export interface UseTTSResult {
	state: TTSState;
	isSpeaking: boolean;
	isConnected: boolean;
	speak: (text: string) => Promise<void>;
	stop: () => void;
	error: string | null;
	preparation: string | null;
	download: TTSDownloadProgress | null;
	settings: TTSSettings;
	availableVoices: string[];
	setProvider: (provider: TTSProvider) => void;
	setLanguage: (language: string) => void;
	setVoice: (voice: string) => Promise<void>;
	setSpeed: (speed: number) => Promise<void>;
}
export interface UseTTSWithParagraphsResult extends UseTTSResult {
	play: () => void;
	previousParagraph: () => void;
	nextParagraph: () => void;
	currentParagraph: number;
	totalParagraphs: number;
	hasPrevious: boolean;
	hasNext: boolean;
	isReading: boolean;
}

/** Lazy per-control playback; provider changes synchronously fence and dispose old work. */
export function useTTS(): UseTTSResult {
	const [state, setState] = useState<TTSState>("idle");
	const [isConnected, setIsConnected] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [preparation, setPreparation] = useState<string | null>(null);
	const [download, setDownload] = useState<TTSDownloadProgress | null>(null);
	const [availableVoices, setAvailableVoices] = useState<string[]>([]);
	const serviceRef = useRef<ReadAloudService | null>(null);
	const connectionRef = useRef<Promise<ReadAloudService> | null>(null);
	const generation = useRef(0);
	const dispose = useCallback(() => {
		generation.current++;
		serviceRef.current?.dispose();
		serviceRef.current = null;
		connectionRef.current = null;
	}, []);
	const stop = useCallback(() => {
		dispose();
		setState("idle");
		setError(null);
		setIsConnected(false);
		setPreparation(null);
		setDownload(null);
		setAvailableVoices([]);
	}, [dispose]);
	const subscribe = useCallback(
		(notify: () => void) =>
			subscribeTTSSettings(() => {
				stop();
				notify();
			}),
		[stop],
	);
	const settings = useSyncExternalStore(
		subscribe,
		loadTTSSettings,
		() => DEFAULT_TTS_SETTINGS,
	);
	useMountEffect(() => () => dispose());

	const ensureConnected = useCallback((): Promise<ReadAloudService> => {
		if (connectionRef.current) return connectionRef.current;
		if (serviceRef.current) return Promise.resolve(serviceRef.current);
		const token = generation.current;
		const current = () => token === generation.current;
		setState("connecting");
		setError(null);
		setPreparation("Preparing read aloud…");
		const service = createReadAloudService(loadTTSSettings(), {
			onPlaying: () => {
				if (current()) setState("speaking");
			},
			onVoicesLoaded: (voices) => {
				if (current()) setAvailableVoices(voices);
			},
			onDownload: (progress) => {
				if (current()) setDownload(progress);
			},
			onPreparation: (label) => {
				if (current()) setPreparation(label);
			},
		});
		serviceRef.current = service;
		const pending = service
			.prepare()
			.then(() => {
				if (!current())
					throw new DOMException("Playback stopped", "AbortError");
				setIsConnected(true);
				setState("idle");
				setPreparation(null);
				setDownload(null);
				return service;
			})
			.catch((failure) => {
				service.dispose();
				if (current()) serviceRef.current = null;
				throw failure;
			})
			.finally(() => {
				if (current()) connectionRef.current = null;
			});
		connectionRef.current = pending;
		return pending;
	}, []);

	const speak = useCallback(
		async (text: string) => {
			if (!text.trim()) return;
			const token = generation.current;
			try {
				const service = await ensureConnected();
				if (token !== generation.current)
					throw new DOMException("Playback stopped", "AbortError");
				await service.speak(text);
				if (token !== generation.current)
					throw new DOMException("Playback stopped", "AbortError");
				setState("idle");
			} catch (failure) {
				if (token === generation.current) {
					dispose();
					setIsConnected(false);
					setPreparation(null);
					setDownload(null);
					setError(
						failure instanceof Error ? failure.message : "Read aloud failed",
					);
					setState("error");
				}
				// Paragraph sequences must stop on failure/cancel, not continue to another destination.
				throw failure;
			}
		},
		[dispose, ensureConnected],
	);
	const setVoice = useCallback(async (voice: string) => {
		const current = loadTTSSettings();
		if (current.provider === "phonon") updatePhononTTSSettings({ voice });
		else saveTTSSettings({ ...current, voice });
	}, []);
	const setSpeed = useCallback(async (speed: number) => {
		if (!Number.isFinite(speed)) throw new Error("Invalid playback speed");
		const clamped = Math.max(0.5, Math.min(2, speed));
		const current = loadTTSSettings();
		if (current.provider === "phonon")
			updatePhononTTSSettings({ speed: clamped });
		else saveTTSSettings({ ...current, speed: clamped });
	}, []);
	const setLanguage = useCallback((language: string) => {
		updatePhononTTSSettings({ language });
	}, []);
	return {
		state,
		isSpeaking: state === "speaking",
		isConnected,
		speak,
		stop,
		error,
		preparation,
		download,
		settings,
		availableVoices,
		setProvider: selectTTSProvider,
		setLanguage,
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
	const [isReading, setIsReading] = useState(false);
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

	const player = useMemo(
		() =>
			createParagraphPlayer(
				paragraphs,
				tts.speak,
				setCurrentIndex,
				(sessionId) => sessionId === sessionIdRef.current,
			),
		[paragraphs, tts.speak],
	);

	const playFrom = useCallback(
		(index: number) => {
			const newSession = ++sessionIdRef.current;
			setIsReading(true);
			player
				.playFrom(index, newSession)
				.then(() => {
					// Keep one engine warm across paragraphs, not one idle model per message.
					if (newSession === sessionIdRef.current) tts.stop();
				})
				.catch((err) => {
					if (
						newSession !== sessionIdRef.current ||
						((err instanceof Error || err instanceof DOMException) &&
							err.name === "AbortError")
					) {
						return;
					}
					console.error("[TTS] speakParagraph error:", err);
				})
				.finally(() => {
					if (newSession === sessionIdRef.current) {
						setIsReading(false);
					}
				});
		},
		[player, tts.stop],
	);

	const play = useCallback(() => {
		if (isReading || tts.isSpeaking) {
			sessionIdRef.current++; // Invalidate current session
			tts.stop();
			setIsReading(false);
		} else {
			playFrom(currentIndex);
		}
	}, [isReading, tts, currentIndex, playFrom]);

	const stop = useCallback(() => {
		sessionIdRef.current++; // Invalidate current session
		tts.stop();
		setIsReading(false);
	}, [tts]);

	const previousParagraph = useCallback(() => {
		if (currentIndex > 0) {
			sessionIdRef.current++;
			tts.stop();
			setIsReading(false);
			const newIndex = currentIndex - 1;
			setCurrentIndex(newIndex);
			playFrom(newIndex);
		}
	}, [currentIndex, playFrom, tts]);

	const nextParagraph = useCallback(() => {
		if (currentIndex < paragraphs.length - 1) {
			sessionIdRef.current++;
			tts.stop();
			setIsReading(false);
			const newIndex = currentIndex + 1;
			setCurrentIndex(newIndex);
			playFrom(newIndex);
		}
	}, [currentIndex, paragraphs.length, playFrom, tts]);

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
		isReading,
	};
}
