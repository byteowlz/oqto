import { useLocalStorage } from "@/hooks/use-local-storage";
import {
	DEFAULT_RECOGNITION_SETTINGS,
	RECOGNITION_SETTINGS_KEY,
	type RecognitionSettings,
	parseRecognitionSettings,
	reportRecognitionStorageError,
} from "@/lib/voice/recognition-settings";
import { STTService } from "@/lib/voice/stt-service";
import type {
	MicrophoneDevice,
	ModelDownloadProgress,
} from "@/lib/voice/stt-service";
import type { WakeRelease } from "@/lib/voice/types";
import type { VoiceConfig } from "@/lib/voice/types";
import { WakeMeter } from "@/lib/voice/wake-meter";
import { WakeService } from "@/lib/voice/wake-service";
import {
	type WakeSound,
	playWakeSound,
	unlockWakeSoundsOnGesture,
} from "@/lib/voice/wake-sounds";
import type {
	RecognitionLanguage,
	RecognitionProvider,
} from "@byteowlz/ears-browser";
import { useCallback, useEffect, useRef, useState } from "react";

export interface UseDictationOptions {
	/** Composer identity; changing it cancels capture and fences queued delivery. */
	scopeKey?: string;
	config: VoiceConfig | null;
	/** Authenticated URL supplied by the host adapter; local recognition never uses it. */
	remoteUrl?: string;
	onTranscript: (text: string) => void;
	vadTimeoutMs?: number;
	/** Called when a pause in speech should send the dictation. */
	onAutoSend?: () => void;
	/** Wake phrases share this hook so the legacy UI and OqtoUI behave identically. */
	wakeRelease?: WakeRelease | null;
}
export interface UseDictationReturn {
	isActive: boolean;
	liveTranscript: string;
	vadProgress: number;
	/** Leaf visualization samples capture without rerendering the whole composer. */
	getInputVolume: () => number;
	isConnected: boolean;
	error: string | null;
	dismissError: () => void;
	preparation: string | null;
	download: ModelDownloadProgress | null;
	recognition: RecognitionSettings;
	remoteAvailable: boolean;
	setRecognitionProvider: (provider: RecognitionProvider) => void;
	setRecognitionLanguage: (language: RecognitionLanguage) => void;
	/** Auto-send is a pause: it sends once speech has stopped for this long. */
	autoSendEnabled: boolean;
	autoSendPauseMs: number;
	setAutoSendPauseMs: (pauseMs: number) => void;
	setAutoSendEnabled: (enabled: boolean) => void;
	/** True only when the server advertises a verified wake release. */
	wakeAvailable: boolean;
	/** "off" | "loading" | "listening" | "muted" | "unavailable" | "error". */
	wakeState: string;
	/** Phrases from the loaded release manifest; empty until it has loaded. */
	wakePhrases: string[];
	setWakeEnabled: (enabled: boolean) => void;
	setWakePhrase: (phrase: string) => void;
	/** Debug view: live score, trigger score and threshold slider. */
	setWakeDebug: (enabled: boolean) => void;
	/** Local threshold override; null restores the release threshold. */
	setWakeThreshold: (threshold: number | null) => void;
	/** Input devices the browser exposes; labels need one granted capture. */
	microphones: MicrophoneDevice[];
	/** Re-enumerate inputs (call when the settings overlay opens). */
	refreshMicrophones: () => Promise<void>;
	setMicrophone: (deviceId: string | null) => void;
	/** Silence after which a dictation ends while wake is on; 0 = never. */
	setWakeSilenceTimeoutMs: (timeoutMs: number) => void;
	/** Choose the sound played on a detection ("off" for none). */
	setWakeSound: (sound: WakeSound) => void;
	/** Choose the sound played right before an auto-send ("off" for none). */
	setSendSound: (sound: WakeSound) => void;
	/** Last voice event worth a reaction (mascot): what happened and when. */
	voiceCue: { kind: "wake" | "sent" | "error"; at: number } | null;
	/** A pending auto-send: when its pause started and how long it lasts. */
	sendCountdown: { startedAt: number; durationMs: number } | null;
	/** Threshold shipped with the loaded release; null until it has loaded. */
	wakeReleaseThreshold: number | null;
	/** Live score store for the debug meter (subscribe directly). */
	wakeMeter: WakeMeter;
	/** Score of the detection that started the current dictation, if any. */
	wakeTriggerScore: number | null;
	/** Half-duplex: call while the app speaks. */
	setWakePlayback: (active: boolean) => void;
	start: () => Promise<void>;
	/** Drain actual engine completion, without promoting a preview. */
	stop: () => void;
	/** Drop provisional speech and pending auto-send. */
	cancel: () => void;
}

type CompletionContext = {
	isCurrent: () => boolean;
	latest: () => {
		options: UseDictationOptions;
		recognition: RecognitionSettings;
	};
	finishing: () => boolean;
	schedule: (action: () => void, delay: number) => void;
	/** Dictation ends after it sent itself, so the wake phrase can listen again. */
	end: () => void;
};

/** Committed delivery is separate from connection wiring and provisional rendering. */
function deliverCompletedTranscript(text: string, context: CompletionContext) {
	if (!context.isCurrent() || !text.trim()) return;
	const current = context.latest();
	current.options.onTranscript(text);
	// Auto-send is a pause after the last speech, not a per-sentence timer: each
	// further final or preview restarts it, so nothing is cut off mid-thought.
	if (
		context.finishing() ||
		!current.recognition.autoSendEnabled ||
		!current.options.onAutoSend
	)
		return;
	const pause = current.recognition.autoSendPauseMs;
	context.schedule(() => {
		if (!context.isCurrent() || !context.latest().recognition.autoSendEnabled)
			return;
		playWakeSound(context.latest().recognition.sendSound);
		context.latest().options.onAutoSend?.();
		context.end();
	}, pause);
}

export function useDictation(options: UseDictationOptions): UseDictationReturn {
	const sttRef = useRef<STTService | null>(null);
	const tokenRef = useRef(0);
	const activeRef = useRef(false);
	const runningScopeRef = useRef<string | undefined>(undefined);
	const runningRecognitionRef = useRef<RecognitionSettings | null>(null);
	const finishingRef = useRef(false);

	const autoSendTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
	// With wake on, a dictation must end by itself or wake can never listen again.
	const silenceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
	const stopRef = useRef<() => void>(() => {});
	const [isActive, setIsActive] = useState(false);
	const [liveTranscript, setLiveTranscript] = useState("");

	const [isConnected, setIsConnected] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [preparation, setPreparation] = useState<string | null>(null);
	const [download, setDownload] = useState<ModelDownloadProgress | null>(null);
	const [recognition, setRecognition] = useLocalStorage(
		RECOGNITION_SETTINGS_KEY,
		DEFAULT_RECOGNITION_SETTINGS,
		{
			deserialize: parseRecognitionSettings,
			onError: reportRecognitionStorageError,
		},
	);
	const autoSendEnabled = recognition.autoSendEnabled;
	const wakeRef = useRef<WakeService | null>(null);
	const wakeKeyRef = useRef("");
	const [wakeState, setWakeState] = useState("off");
	const [wakePhrases, setWakePhrases] = useState<string[]>([]);
	const [wakeReleaseThreshold, setWakeReleaseThreshold] = useState<
		number | null
	>(null);
	const [wakeTriggerScore, setWakeTriggerScore] = useState<number | null>(null);
	const pendingWakeScoreRef = useRef<number | null>(null);
	const [microphones, setMicrophones] = useState<MicrophoneDevice[]>([]);
	const [voiceCue, setVoiceCue] =
		useState<UseDictationReturn["voiceCue"]>(null);
	const [sendCountdown, setSendCountdown] = useState<{
		startedAt: number;
		durationMs: number;
	} | null>(null);
	const wakeMeterRef = useRef<WakeMeter | null>(null);
	wakeMeterRef.current ??= new WakeMeter();
	const wakeMeter = wakeMeterRef.current;
	const latest = useRef({ options, recognition });
	latest.current = { options, recognition };

	// Patching settings must not cancel a running dictation (see tuneWake).
	const patchRecognition = useCallback(
		(patch: Partial<RecognitionSettings>) => {
			const next = { ...latest.current.recognition, ...patch };
			latest.current.recognition = next;
			setRecognition(next);
		},
		[setRecognition],
	);
	// Enumeration is read-only and needs no capture; labels appear once the
	// browser has granted the microphone at least once.
	const refreshMicrophones = useCallback(async () => {
		const service = new STTService("", 0, latest.current.recognition);
		try {
			const found = await service.listMicrophones();
			// An unchanged list keeps its identity: no re-render for nothing.
			setMicrophones((current) =>
				current.length === found.length &&
				current.every(
					(device, index) =>
						device.deviceId === found[index]?.deviceId &&
						device.label === found[index]?.label,
				)
					? current
					: found,
			);
		} catch {
			setMicrophones((current) => (current.length ? [] : current));
		} finally {
			service.disconnect();
		}
	}, []);
	const cancelRef = useRef<(() => void) | null>(null);
	const setMicrophone = useCallback(
		(microphoneId: string | null) => {
			// Changing the input must not leave a capture open on the old device.
			cancelRef.current?.();
			patchRecognition({ microphoneId });
		},
		[patchRecognition],
	);
	const clearSilence = useCallback(() => {
		if (silenceTimer.current !== null) clearTimeout(silenceTimer.current);
		silenceTimer.current = null;
	}, []);
	/** (Re)start the silence window; speech calls this again to extend it. */
	const armSilence = useCallback(() => {
		clearSilence();
		const { recognition: current } = latest.current;
		if (
			!activeRef.current ||
			finishingRef.current ||
			!current.wakeEnabled ||
			current.wakeSilenceTimeoutMs <= 0
		)
			return;
		silenceTimer.current = setTimeout(() => {
			silenceTimer.current = null;
			// Finish, not cancel: completed speech is delivered and kept.
			stopRef.current();
		}, current.wakeSilenceTimeoutMs);
	}, [clearSilence]);
	const clearAutoSend = useCallback(() => {
		if (autoSendTimer.current !== null) clearTimeout(autoSendTimer.current);
		autoSendTimer.current = null;
		setSendCountdown(null);
	}, []);
	const cancel = useCallback(() => {
		tokenRef.current++;
		activeRef.current = false;
		runningScopeRef.current = undefined;
		runningRecognitionRef.current = null;
		finishingRef.current = false;
		clearAutoSend();
		clearSilence();
		sttRef.current?.disconnect();
		sttRef.current = null;
		setIsActive(false);
		setIsConnected(false);
		setPreparation(null);
		setDownload(null);
		setLiveTranscript("");
		setWakeTriggerScore(null);
	}, [clearAutoSend, clearSilence]);
	// Settings changes must not cancel a running dictation (see tuneWake) - except
	// the microphone, which is a device the old capture still holds.
	const setAutoSendEnabled = useCallback(
		(enabled: boolean) => {
			patchRecognition({ autoSendEnabled: enabled });
			if (!enabled) clearAutoSend();
		},
		[clearAutoSend, patchRecognition],
	);
	const setAutoSendPauseMs = useCallback(
		(pauseMs: number) => patchRecognition({ autoSendPauseMs: pauseMs }),
		[patchRecognition],
	);
	const setWakeSound = useCallback(
		(wakeSound: WakeSound) => {
			patchRecognition({ wakeSound });
			// Choosing is a user gesture: let the user hear it right away.
			playWakeSound(wakeSound);
		},
		[patchRecognition],
	);
	const setSendSound = useCallback(
		(sendSound: WakeSound) => {
			patchRecognition({ sendSound });
			playWakeSound(sendSound);
		},
		[patchRecognition],
	);
	const setWakeSilenceTimeoutMs = useCallback(
		(timeoutMs: number) => {
			patchRecognition({ wakeSilenceTimeoutMs: timeoutMs });
			armSilence();
		},
		[patchRecognition, armSilence],
	);

	cancelRef.current = cancel;

	// Cancelling after an auto-send keeps the delivered text and lets the wake
	// phrase listen again; it must not go through stop(), which would re-finalise.
	const endRef = useRef<() => void>(() => {});
	endRef.current = () => cancel();

	const start = useCallback(async () => {
		if (activeRef.current) return;
		cancel();
		setWakeTriggerScore(pendingWakeScoreRef.current);
		pendingWakeScoreRef.current = null;
		const token = tokenRef.current;
		const { recognition: selected, options: current } = latest.current;
		setError(null);
		setPreparation("Preparing speech recognition…");
		setIsActive(true);
		activeRef.current = true;
		runningScopeRef.current = current.scopeKey;
		runningRecognitionRef.current = selected;
		try {
			if (
				selected.provider === "ears" &&
				(!current.config?.stt_url || !current.remoteUrl)
			)
				throw new Error("Remote eaRS recognition is not configured");
			const service = new STTService(
				selected.provider === "ears" ? (current.remoteUrl ?? "") : "",
				current.vadTimeoutMs ?? current.config?.vad_timeout_ms ?? 1500,
				selected,
			);
			if (selected.microphoneId) service.setMicrophone(selected.microphoneId);
			sttRef.current = service;
			const isCurrent = () =>
				token === tokenRef.current &&
				activeRef.current &&
				current.scopeKey === latest.current.options.scopeKey;
			service.setCallbacks({
				onDownload: (progress) => {
					if (isCurrent()) setDownload(progress);
				},
				onPreview: (text) => {
					if (!isCurrent()) return;
					setLiveTranscript(text);
					// New speech restarts the pause, so a sentence break never sends.
					if (text.trim()) {
						clearAutoSend();
						armSilence();
					}
				},
				onPreparation: (label) => {
					if (isCurrent()) setPreparation(label);
				},
				onConnectionChange: (connected) => {
					if (isCurrent()) setIsConnected(connected);
				},
				onFinal: (text) => {
					if (text.trim()) armSilence();
					deliverCompletedTranscript(text, {
						isCurrent,
						latest: () => latest.current,
						finishing: () => finishingRef.current,
						end: () => {
							setVoiceCue({ kind: "sent", at: Date.now() });
							endRef.current();
						},
						schedule: (action, delay) => {
							clearAutoSend();
							setSendCountdown({ startedAt: Date.now(), durationMs: delay });
							autoSendTimer.current = setTimeout(() => {
								autoSendTimer.current = null;
								setSendCountdown(null);
								action();
							}, delay);
						},
					});
				},
				onError: (message) => {
					if (!isCurrent()) return;
					cancel();
					setError(message);
					setVoiceCue({ kind: "error", at: Date.now() });
				},
			});
			await service.startListening();
			if (!isCurrent()) return;
			setPreparation(null);
			// The silence window starts once the microphone is actually live.
			armSilence();
		} catch (failure) {
			if (token !== tokenRef.current) return;
			cancel();
			setError(
				failure instanceof Error
					? failure.message
					: "Speech recognition failed",
			);
		}
	}, [cancel, clearAutoSend, armSilence]);

	const stop = useCallback(() => {
		if (!activeRef.current || finishingRef.current) return;
		finishingRef.current = true;
		clearAutoSend();
		clearSilence();
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
	}, [cancel, clearAutoSend, clearSilence]);
	stopRef.current = stop;

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
	const setWakePhrase = useCallback(
		(wakePhrase: string) => changeRecognition({ wakePhrase }),
		[changeRecognition],
	);
	const setWakeEnabled = useCallback(
		(wakeEnabled: boolean) => changeRecognition({ wakeEnabled }),
		[changeRecognition],
	);
	// Debug tuning must not cancel a running dictation or re-arm the microphone.
	const tuneWake = useCallback(
		(
			patch: Pick<Partial<RecognitionSettings>, "wakeDebug" | "wakeThreshold">,
		) => {
			patchRecognition(patch);
			if ("wakeThreshold" in patch)
				wakeRef.current?.setThreshold(patch.wakeThreshold ?? null);
		},
		[patchRecognition],
	);
	const setWakeDebug = useCallback(
		(wakeDebug: boolean) => tuneWake({ wakeDebug }),
		[tuneWake],
	);
	const setWakeThreshold = useCallback(
		(wakeThreshold: number | null) => tuneWake({ wakeThreshold }),
		[tuneWake],
	);
	const setWakePlayback = useCallback((active: boolean) => {
		wakeRef.current?.setPlayback(active);
	}, []);

	// useeffect-guardrail: allow — wake listening is a device resource keyed to these inputs; the effect owns its lifecycle.
	useEffect(() => {
		const release = options.wakeRelease;
		const key = release ? `${release.manifest_url}\n${release.files_url}` : "";
		const enabled = recognition.wakeEnabled && Boolean(release);
		// One verified engine per release: a dictation only borrows the microphone.
		if (wakeRef.current && (!enabled || wakeKeyRef.current !== key)) {
			void wakeRef.current.disarm();
			wakeRef.current = null;
		}
		if (!enabled || !release) {
			setWakeState("off");
			wakeMeter.idle();
			return;
		}
		let service = wakeRef.current;
		if (!service) {
			// Wake can arm straight from a reload, before any click; unlock audio
			// on the first gesture so the detection sound can play. One-shot.
			unlockWakeSoundsOnGesture();
			const created: WakeService = new WakeService(
				{ manifestUrl: release.manifest_url, filesUrl: release.files_url },
				{
					onState: (state) => {
						if (wakeRef.current !== created) return;
						setWakeState(state);
						if (state === "listening" || state === "muted") {
							setWakePhrases(created.phrases);
							setWakeReleaseThreshold(created.getManifest()?.threshold ?? null);
						}
						if (state !== "listening") wakeMeter.idle();
					},
					onError: (message) => {
						if (wakeRef.current === created) setError(message);
					},
					onScores: (scores) => {
						const current = latest.current.recognition;
						if (current.wakeDebug)
							wakeMeter.push(scores[current.wakePhrase] ?? 0);
					},
					onDetection: (phrase, score) => {
						const current = latest.current.recognition;
						if (phrase !== current.wakePhrase) return;
						wakeMeter.trigger(score);
						pendingWakeScoreRef.current = score;
						// Confirm "I heard you" before capture starts; capture setup takes
						// longer than the sound, and echo cancellation covers the overlap.
						playWakeSound(current.wakeSound);
						setVoiceCue({ kind: "wake", at: Date.now() });
						// A detection does exactly what the microphone button does.
						void startRef.current?.();
					},
				},
			);
			created.setThreshold(latest.current.recognition.wakeThreshold);
			wakeRef.current = created;
			wakeKeyRef.current = key;
			service = created;
		}
		if (isActive) void service.disarm();
		else void service.arm();
	}, [options.wakeRelease, recognition.wakeEnabled, isActive, wakeMeter]);

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

	const startRef = useRef<(() => Promise<void>) | null>(null);
	startRef.current = start;

	const getInputVolume = useCallback(
		() => sttRef.current?.getInputVolume() ?? 0,
		[],
	);
	// useeffect-guardrail: allow — release capture and fence callbacks on unmount.
	useEffect(
		() => () => {
			tokenRef.current++;
			activeRef.current = false;
			if (autoSendTimer.current !== null) clearTimeout(autoSendTimer.current);
			sttRef.current?.disconnect();
			void wakeRef.current?.disarm();
		},
		[],
	);

	return {
		isActive,
		liveTranscript,
		vadProgress: 0,
		getInputVolume,
		isConnected,
		error,
		dismissError: () => setError(null),
		preparation,
		download,
		recognition,
		remoteAvailable: Boolean(options.config?.stt_url && options.remoteUrl),
		setRecognitionProvider,
		setRecognitionLanguage,
		autoSendEnabled,
		autoSendPauseMs: recognition.autoSendPauseMs,
		setAutoSendEnabled,
		setAutoSendPauseMs,
		wakeAvailable: Boolean(options.wakeRelease),
		wakeState,
		wakePhrases,
		microphones,
		refreshMicrophones,
		setMicrophone,
		setWakeSilenceTimeoutMs,
		setWakeSound,
		setSendSound,
		sendCountdown,
		voiceCue,
		setWakeEnabled,
		setWakePhrase,
		setWakeDebug,
		setWakeThreshold,
		wakeReleaseThreshold,
		wakeMeter,
		wakeTriggerScore,
		setWakePlayback,
		start,
		stop,
		cancel,
	};
}
