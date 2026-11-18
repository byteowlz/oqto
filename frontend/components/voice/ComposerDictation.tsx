import type { UseDictationReturn } from "@/features/voice/hooks/useDictation";
import { cn } from "@/lib/utils";
import { AUTO_SEND_PAUSES_MS } from "@/lib/voice/recognition-settings";
import { WAKE_SOUNDS, type WakeSound } from "@/lib/voice/wake-sounds";
import { AudioLines, Check, Mic, X } from "lucide-react";
import { useTheme } from "next-themes";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { OqtopusMascot, useOqtopusAvailable } from "./OqtopusMascot";
import { RecognitionControls } from "./RecognitionControls";
import "./composer-dictation.css";

/** Oqto consumes cumulative SDK bytes; never sum per-file snapshots twice. */
export function downloadDisplay(loaded: number, total?: number) {
	const bytes = Number.isFinite(loaded) && loaded >= 0 ? loaded : 0;
	const known =
		typeof total === "number" &&
		Number.isFinite(total) &&
		total > 0 &&
		total >= bytes;
	return {
		loaded: bytes,
		total: known ? total : undefined,
		percent: known ? Math.floor((bytes / total) * 100) : undefined,
	};
}

function SpeechLevel({
	active,
	readLevel,
}: { active: boolean; readLevel: () => number }) {
	const element = useRef<HTMLSpanElement>(null);
	// useeffect-guardrail: allow — leaf-only audio meter owns a bounded timer; no chat-wide React updates.
	useEffect(() => {
		let smooth = 0;
		const update = () => {
			const raw = active ? readLevel() : 0;
			smooth += ((Number.isFinite(raw) ? raw : 0) - smooth) * 0.3;
			element.current?.style.setProperty(
				"--speech-level",
				String(Math.max(0.05, Math.min(1, smooth * 4))),
			);
		};
		update();
		if (!active) return;
		const timer = setInterval(update, 66);
		return () => clearInterval(timer);
	}, [active, readLevel]);
	return (
		<span
			ref={element}
			className="composer-dictation__meter"
			aria-hidden="true"
		>
			{[0, 1, 2, 3, 4].map((bar) => (
				<i key={bar} />
			))}
		</span>
	);
}

/** An anchored status surface: never replaces the editor or participates in layout. */
export function DictationStatus({
	dictation,
	onCancel,
}: {
	dictation: UseDictationReturn;
	onCancel?: () => void;
}) {
	const { i18n, t } = useTranslation();
	const de = i18n.resolvedLanguage?.startsWith("de");
	const key = (name: string) => `voice.dictationSettings.${name}`;
	if (!dictation.isActive && !dictation.error) return null;
	const progress = dictation.download
		? downloadDisplay(dictation.download.loaded, dictation.download.total)
		: null;
	const initialized = progress?.percent === 100;
	const finishing = dictation.preparation === "Finishing speech recognition…";
	const permission =
		dictation.preparation === "Waiting for microphone permission…";
	const label = dictation.error
		? de
			? "Spracherkennung fehlgeschlagen"
			: "Recognition failed"
		: finishing
			? de
				? "Wird abgeschlossen…"
				: "Finishing…"
			: permission
				? de
					? "Mikrofonfreigabe erforderlich…"
					: "Waiting for microphone…"
				: initialized
					? de
						? "Modell wird initialisiert…"
						: "Initializing model…"
					: progress
						? de
							? "Sprachmodell herunterladen"
							: "Downloading speech model"
						: dictation.preparation
							? de
								? "Spracherkennung vorbereiten…"
								: "Preparing recognition…"
							: de
								? "Hört zu"
								: "Listening";
	const format = new Intl.NumberFormat(de ? "de" : "en", {
		maximumFractionDigits: 1,
	});
	const details = progress
		? `${format.format(progress.loaded / 1e6)}${progress.total ? ` / ${format.format(progress.total / 1e6)}` : ""} MB`
		: null;
	return (
		<div
			className="composer-dictation"
			data-state={
				dictation.error
					? "error"
					: dictation.preparation
						? "preparing"
						: "listening"
			}
		>
			<div className="composer-dictation__strip">
				<SpeechLevel
					active={dictation.isActive && !dictation.preparation}
					readLevel={dictation.getInputVolume}
				/>
				<output className="composer-dictation__label">{label}</output>
				<span className="composer-dictation__badges">
					{dictation.recognition.language.toUpperCase()} ·{" "}
					{dictation.recognition.provider === "moonshine"
						? t(key("providerBadgeLocal"))
						: t(key("providerBadgeRemote"))}
					{dictation.autoSendEnabled ? " · Auto" : ""}
					{dictation.recognition.wakeDebug &&
					dictation.wakeTriggerScore !== null
						? ` · ${t(key("wakeTriggerBadge"))} ${dictation.wakeTriggerScore.toFixed(2)}`
						: ""}
				</span>
				{dictation.error ? (
					<button
						className="composer-dictation__action"
						type="button"
						onClick={() => void dictation.start()}
					>
						{de ? "Erneut" : "Retry"}
					</button>
				) : (
					<button
						className="composer-dictation__action"
						type="button"
						disabled={Boolean(dictation.preparation)}
						onClick={dictation.stop}
					>
						<Check aria-hidden="true" />
						{de ? "Fertig" : "Finish"}
					</button>
				)}
				<button
					className="composer-dictation__action composer-dictation__cancel"
					type="button"
					aria-label={de ? "Diktieren abbrechen" : "Cancel dictation"}
					onClick={() => {
						(onCancel ?? dictation.cancel)();
						dictation.dismissError();
					}}
				>
					<X aria-hidden="true" />
				</button>
			</div>
			<div className="composer-dictation__body">
				{dictation.error ? (
					<span role="alert" className="composer-dictation__error">
						{dictation.error}
					</span>
				) : dictation.sendCountdown ? (
					<>
						<span className="composer-dictation__preview">
							{dictation.liveTranscript || t(key("sendingSoon"))}
						</span>
						<div
							className="composer-dictation__send-countdown"
							role="timer"
							aria-label={t(key("sendCountdown"))}
						>
							{/* key restarts the animation each time speech resets the pause */}
							<i
								key={dictation.sendCountdown.startedAt}
								style={{
									animationDuration: `${dictation.sendCountdown.durationMs}ms`,
								}}
							/>
						</div>
					</>
				) : progress || dictation.preparation ? (
					<>
						<div className="composer-dictation__download-meta">
							<span>
								{details ??
									(de
										? "Audio bleibt beim gewählten Anbieter."
										: "Using your selected speech provider.")}
							</span>
							<span>
								{progress?.percent !== undefined ? `${progress.percent}%` : ""}
							</span>
						</div>
						<progress
							className="composer-dictation__progress"
							aria-label={
								de ? "Sprachmodell vorbereiten" : "Speech model preparation"
							}
							max={100}
							value={progress?.percent}
						/>
					</>
				) : (
					<span
						className="composer-dictation__preview"
						aria-label={
							de ? "Vorläufiges Transkript" : "Provisional transcript"
						}
					>
						{dictation.liveTranscript ||
							(de
								? "Sprich los. Fertige Sätze erscheinen im Entwurf."
								: "Speak naturally. Completed speech appears in your draft.")}
					</span>
				)}
			</div>
		</div>
	);
}

/** Opens the dictation settings overlay above the prompt box. */
export function DictationMicButton({
	dictation,
	className,
	settingsOpen = false,
	onToggleSettings,
	panelId,
}: {
	dictation: UseDictationReturn;
	className?: string;
	settingsOpen?: boolean;
	onToggleSettings?: () => void;
	panelId?: string;
}) {
	const { i18n, t } = useTranslation();
	const de = i18n.resolvedLanguage?.startsWith("de");
	const key = (name: string) => `voice.dictationSettings.${name}`;
	// Wake listening keeps a microphone open; the icon must say so (oqto-623a).
	const wake =
		!dictation.isActive &&
		dictation.recognition.wakeEnabled &&
		["loading", "listening", "muted", "error"].includes(dictation.wakeState)
			? dictation.wakeState
			: null;
	const wakeId = useId();
	const { resolvedTheme } = useTheme();
	// With wake on, the Oqtopus is the listening indicator: it breathes while
	// waiting, comes alive while dictating, and reacts to what happens.
	const mascot = useOqtopusAvailable(dictation.recognition.wakeEnabled);
	const phrase = dictation.recognition.wakePhrase.replaceAll("_", " ");
	const wakeLabel =
		wake === "listening"
			? de
				? `Hört auf „${phrase}“`
				: `Listening for "${phrase}"`
			: wake === "muted"
				? de
					? "Weckwort pausiert, während Oqto spricht"
					: "Wake phrase paused while Oqto speaks"
				: wake === "loading"
					? de
						? "Weckwort wird geladen"
						: "Loading wake phrase"
					: wake === "error"
						? de
							? "Weckwort nicht verfügbar"
							: "Wake phrase unavailable"
						: null;
	return (
		<button
			type="button"
			onClick={() => {
				// Re-enumerate on open: inputs change (headsets, Continuity, virtual mics).
				// Done here, not in an effect, so it runs once per open and never loops.
				if (!settingsOpen) void dictation.refreshMicrophones();
				onToggleSettings?.();
			}}
			className={cn(
				"composer-dictation-mic",
				dictation.isActive && "composer-dictation-mic--active",
				className,
			)}
			aria-label={t(key("dictationSettings"))}
			aria-pressed={dictation.isActive}
			aria-expanded={settingsOpen}
			aria-controls={settingsOpen ? panelId : undefined}
			aria-describedby={wakeLabel ? wakeId : undefined}
			title={wakeLabel ?? undefined}
			data-wake={wake ?? undefined}
			data-mascot={mascot ? "" : undefined}
		>
			{mascot ? (
				<OqtopusMascot
					lively={dictation.isActive}
					cue={dictation.voiceCue}
					theme={resolvedTheme === "light" ? "light" : "dark"}
				/>
			) : dictation.isActive ? (
				<AudioLines aria-hidden="true" />
			) : (
				<Mic aria-hidden="true" />
			)}
			{/* The mascot shows "listening" itself; the dot keeps the states it cannot. */}
			{wake && !(mascot && wake === "listening") ? (
				<span aria-hidden="true" className="composer-dictation-mic__wake" />
			) : null}
			{wakeLabel ? (
				<span id={wakeId} className="sr-only">
					{wakeLabel}
				</span>
			) : null}
		</button>
	);
}

/**
 * Dictation settings as an overlay above the prompt box rather than a modal, so
 * the score meter and threshold stay visible while dictating (oqto-623a). It is
 * not modal: no focus trap, Escape or the microphone button closes it.
 */
export function DictationSettingsPanel({
	dictation,
	open,
	onClose,
	className,
}: {
	dictation: UseDictationReturn;
	open: boolean;
	onClose: () => void;
	className?: string;
}) {
	const { t } = useTranslation();
	const key = (name: string) => `voice.dictationSettings.${name}`;
	const panelId = useId();
	if (!open) return null;
	return (
		<section
			id={panelId}
			aria-label={t(key("dictationSettings"))}
			className={cn("composer-dictation-settings", className)}
			onKeyDown={(event) => {
				if (event.key === "Escape") onClose();
			}}
		>
			<div className="composer-dictation-settings__head">
				<span>{t(key("dictationSettings"))}</span>
				<button
					type="button"
					className="composer-dictation-settings__close"
					aria-label={t(key("finish"))}
					onClick={onClose}
				>
					<X aria-hidden="true" />
				</button>
			</div>
			<RecognitionControls
				recognition={dictation.recognition}
				remoteAvailable={dictation.remoteAvailable}
				onProviderChange={dictation.setRecognitionProvider}
				onLanguageChange={dictation.setRecognitionLanguage}
				wakeAvailable={dictation.wakeAvailable}
				wakeEnabled={dictation.recognition.wakeEnabled}
				onWakeChange={dictation.setWakeEnabled}
				wakePhrases={dictation.wakePhrases}
				wakePhrase={dictation.recognition.wakePhrase}
				onWakePhraseChange={dictation.setWakePhrase}
				microphones={dictation.microphones}
				onMicrophoneChange={dictation.setMicrophone}
				onWakeSilenceTimeoutChange={dictation.setWakeSilenceTimeoutMs}
				onWakeSoundChange={dictation.setWakeSound}
				wakeState={dictation.wakeState}
				wakeMeter={dictation.wakeMeter}
				onWakeDebugChange={dictation.setWakeDebug}
				wakeReleaseThreshold={dictation.wakeReleaseThreshold}
				onWakeThresholdChange={dictation.setWakeThreshold}
				disabled={dictation.isActive}
			/>
			<label className="composer-dictation-settings__auto">
				<input
					type="checkbox"
					checked={dictation.autoSendEnabled}
					onChange={(event) =>
						dictation.setAutoSendEnabled(event.target.checked)
					}
				/>
				{t(key("autoSend"))}
			</label>
			{dictation.autoSendEnabled ? (
				<label className="composer-dictation-settings__pause">
					<span>{t(key("sendSound"))}</span>
					<select
						value={dictation.recognition.sendSound}
						onChange={(event) =>
							dictation.setSendSound(event.target.value as WakeSound)
						}
					>
						{WAKE_SOUNDS.map((sound) => (
							<option key={sound} value={sound}>
								{t(key(`wakeSound_${sound}`))}
							</option>
						))}
					</select>
				</label>
			) : null}
			{dictation.autoSendEnabled ? (
				<label className="composer-dictation-settings__pause">
					<span>{t(key("autoSendPause"))}</span>
					<select
						value={dictation.autoSendPauseMs}
						onChange={(event) =>
							dictation.setAutoSendPauseMs(Number(event.target.value))
						}
					>
						{AUTO_SEND_PAUSES_MS.map((pause) => (
							<option key={pause} value={pause}>
								{pause / 1000} {t(key("seconds"))}
							</option>
						))}
					</select>
				</label>
			) : null}
			<p className="composer-dictation-settings__hint">
				{dictation.autoSendEnabled
					? t(key("autoSendHint"), { pause: dictation.autoSendPauseMs / 1000 })
					: t(key("autoSendOff"))}
			</p>
			<button
				type="button"
				className="composer-dictation-settings__start"
				disabled={dictation.isActive}
				onClick={() => void dictation.start()}
			>
				{t(key("start"))}
			</button>
		</section>
	);
}
