import { useLocalStorage } from "@/hooks/use-local-storage";
import { voiceProxyWsUrl } from "@/lib/control-plane-client";
import { STTService } from "@/lib/voice";
import {
	DEFAULT_RECOGNITION_SETTINGS,
	RECOGNITION_SETTINGS_KEY,
	type RecognitionSettings,
	parseRecognitionSettings,
	reportRecognitionStorageError,
} from "@/lib/voice/recognition-settings";
import type { VoiceConfig } from "@/lib/voice/types";
import type {
	RecognitionLanguage,
	RecognitionProvider,
} from "@byteowlz/ears-browser";
import { useCallback, useEffect, useRef, useState } from "react";

export interface UseDictationOptions {
	/** Composer identity; changing it cancels capture and fences queued delivery. */
	scopeKey?: string;
	config: VoiceConfig | null;
	onTranscript: (text: string) => void;
	vadTimeoutMs?: number;
	autoSendOnFinal?: boolean;
	autoSendDelayMs?: number;
	onAutoSend?: () => void;
}
export interface UseDictationReturn {
	isActive: boolean;
	liveTranscript: string;
	vadProgress: number;
	inputVolume: number;
	isConnected: boolean;
	error: string | null;
	preparation: string | null;
	recognition: RecognitionSettings;
	remoteAvailable: boolean;
	setRecognitionProvider: (provider: RecognitionProvider) => void;
	setRecognitionLanguage: (language: RecognitionLanguage) => void;
	autoSendEnabled: boolean;
	setAutoSendEnabled: (enabled: boolean) => void;
	start: () => Promise<void>;
	/** Drain actual engine completion, without promoting a preview. */
	stop: () => void;
	/** Drop provisional speech and pending auto-send. */
	cancel: () => void;
}

type CompletionContext = {
	isCurrent: () => boolean;
	latest: () => { options: UseDictationOptions; autoSendEnabled: boolean };
	finishing: () => boolean;
	schedule: (action: () => void, delay: number) => void;
};

/** Committed delivery is separate from connection wiring and provisional rendering. */
function deliverCompletedTranscript(text: string, context: CompletionContext) {
	if (!context.isCurrent() || !text.trim()) return;
	const current = context.latest();
	current.options.onTranscript(text);
	if (
		context.finishing() ||
		!current.autoSendEnabled ||
		!current.options.onAutoSend
	)
		return;
	context.schedule(() => {
		if (context.isCurrent() && context.latest().autoSendEnabled)
			context.latest().options.onAutoSend?.();
	}, current.options.autoSendDelayMs ?? 0);
}

export function useDictation(options: UseDictationOptions): UseDictationReturn {
	const sttRef = useRef<STTService | null>(null);
	const tokenRef = useRef(0);
	const activeRef = useRef(false);
	const runningScopeRef = useRef<string | undefined>(undefined);
	const runningRecognitionRef = useRef<RecognitionSettings | null>(null);
	const finishingRef = useRef(false);
	const smoothVolumeRef = useRef(0);
	const autoSendTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
	const [isActive, setIsActive] = useState(false);
	const [liveTranscript, setLiveTranscript] = useState("");
	const [inputVolume, setInputVolume] = useState(0);
	const [isConnected, setIsConnected] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [preparation, setPreparation] = useState<string | null>(null);
	const [recognition, setRecognition] = useLocalStorage(
		RECOGNITION_SETTINGS_KEY,
		DEFAULT_RECOGNITION_SETTINGS,
		{
			deserialize: parseRecognitionSettings,
			onError: reportRecognitionStorageError,
		},
	);
	const [autoSendEnabled, setAutoSend] = useState(
		options.autoSendOnFinal ?? false,
	);
	const latest = useRef({ options, autoSendEnabled, recognition });
	latest.current = { options, autoSendEnabled, recognition };

	const clearAutoSend = useCallback(() => {
		if (autoSendTimer.current !== null) clearTimeout(autoSendTimer.current);
		autoSendTimer.current = null;
	}, []);
	const cancel = useCallback(() => {
		tokenRef.current++;
		activeRef.current = false;
		runningScopeRef.current = undefined;
		runningRecognitionRef.current = null;
		finishingRef.current = false;
		clearAutoSend();
		sttRef.current?.disconnect();
		sttRef.current = null;
		setIsActive(false);
		setIsConnected(false);
		setPreparation(null);
		setLiveTranscript("");
	}, [clearAutoSend]);
	const setAutoSendEnabled = useCallback(
		(enabled: boolean) => {
			latest.current.autoSendEnabled = enabled;
			setAutoSend(enabled);
			if (!enabled) clearAutoSend();
		},
		[clearAutoSend],
	);

	const start = useCallback(async () => {
		if (activeRef.current) return;
		cancel();
		const token = tokenRef.current;
		const { recognition: selected, options: current } = latest.current;
		setError(null);
		setPreparation("Preparing speech recognition…");
		setIsActive(true);
		activeRef.current = true;
		runningScopeRef.current = current.scopeKey;
		runningRecognitionRef.current = selected;
		try {
			if (selected.provider === "ears" && !current.config?.stt_url)
				throw new Error("Remote eaRS recognition is not configured");
			const service = new STTService(
				selected.provider === "ears" ? voiceProxyWsUrl("stt") : "",
				current.vadTimeoutMs ?? current.config?.vad_timeout_ms ?? 1500,
				selected,
			);
			sttRef.current = service;
			const isCurrent = () =>
				token === tokenRef.current &&
				activeRef.current &&
				current.scopeKey === latest.current.options.scopeKey;
			service.setCallbacks({
				onPreview: (text) => {
					if (isCurrent()) setLiveTranscript(text);
				},
				onPreparation: (label) => {
					if (isCurrent()) setPreparation(label);
				},
				onConnectionChange: (connected) => {
					if (isCurrent()) setIsConnected(connected);
				},
				onFinal: (text) =>
					deliverCompletedTranscript(text, {
						isCurrent,
						latest: () => latest.current,
						finishing: () => finishingRef.current,
						schedule: (action, delay) => {
							clearAutoSend();
							autoSendTimer.current = setTimeout(() => {
								autoSendTimer.current = null;
								action();
							}, delay);
						},
					}),
				onError: (message) => {
					if (!isCurrent()) return;
					cancel();
					setError(message);
				},
			});
			await service.startListening();
			if (!isCurrent()) return;
			setPreparation(null);
		} catch (failure) {
			if (token !== tokenRef.current) return;
			cancel();
			setError(
				failure instanceof Error
					? failure.message
					: "Speech recognition failed",
			);
		}
	}, [cancel, clearAutoSend]);

	const stop = useCallback(() => {
		if (!activeRef.current || finishingRef.current) return;
		finishingRef.current = true;
		clearAutoSend();
		const token = tokenRef.current;
		setPreparation("Finishing speech recognition…");
		void sttRef.current
			?.finishListening()
			.catch((failure) => {
				if (token === tokenRef.current)
					setError(
						failure instanceof Error
							? failure.message
							: "Speech recognition failed",
					);
			})
			.finally(() => {
				if (token === tokenRef.current) cancel();
			});
	}, [cancel, clearAutoSend]);

	const changeRecognition = useCallback(
		(patch: Partial<RecognitionSettings>) => {
			cancel();
			const next = { ...latest.current.recognition, ...patch };
			latest.current.recognition = next;
			setRecognition(next);
			setError(null);
		},
		[cancel, setRecognition],
	);
	const setRecognitionProvider = useCallback(
		(provider: RecognitionProvider) => changeRecognition({ provider }),
		[changeRecognition],
	);
	const setRecognitionLanguage = useCallback(
		(language: RecognitionLanguage) => changeRecognition({ language }),
		[changeRecognition],
	);

	// useeffect-guardrail: allow — composer identity changes release the old capture; callbacks also fence synchronously.
	useEffect(() => {
		if (activeRef.current && runningScopeRef.current !== options.scopeKey)
			cancel();
	}, [options.scopeKey, cancel]);

	// useeffect-guardrail: allow — cross-tab settings changes must stop the old audio destination.
	useEffect(() => {
		const running = runningRecognitionRef.current;
		if (
			running &&
			(running.provider !== recognition.provider ||
				running.language !== recognition.language)
		)
			cancel();
	}, [recognition.provider, recognition.language, cancel]);

	// useeffect-guardrail: allow — microphone visualization owns this bounded timer.
	useEffect(() => {
		if (!isActive) {
			smoothVolumeRef.current = 0;
			setInputVolume(0);
			return;
		}
		const timer = setInterval(() => {
			const volume = sttRef.current?.getInputVolume() ?? 0;
			smoothVolumeRef.current += (volume - smoothVolumeRef.current) * 0.3;
			setInputVolume(
				smoothVolumeRef.current < 0.001 ? 0 : smoothVolumeRef.current,
			);
		}, 66);
		return () => clearInterval(timer);
	}, [isActive]);
	// useeffect-guardrail: allow — release capture and fence callbacks on unmount.
	useEffect(
		() => () => {
			tokenRef.current++;
			activeRef.current = false;
			if (autoSendTimer.current !== null) clearTimeout(autoSendTimer.current);
			sttRef.current?.disconnect();
		},
		[],
	);

	return {
		isActive,
		liveTranscript,
		vadProgress: 0,
		inputVolume,
		isConnected,
		error,
		preparation,
		recognition,
		remoteAvailable: Boolean(options.config?.stt_url),
		setRecognitionProvider,
		setRecognitionLanguage,
		autoSendEnabled,
		setAutoSendEnabled,
		start,
		stop,
		cancel,
	};
}
