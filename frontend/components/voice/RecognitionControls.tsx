import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import {
	type RecognitionSettings,
	WAKE_SILENCE_TIMEOUTS_MS,
} from "@/lib/voice/recognition-settings";
import type { MicrophoneDevice } from "@/lib/voice/stt-service";
import type { WakeMeter } from "@/lib/voice/wake-meter";
import {
	WAKE_SOUNDS,
	type WakeSound,
	playWakeSound,
} from "@/lib/voice/wake-sounds";
import type {
	RecognitionLanguage,
	RecognitionProvider,
} from "@byteowlz/ears-browser";
import { Info, Volume2 } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { WakeDebugPanel } from "./WakeDebugPanel";

export interface RecognitionControlsProps {
	recognition: RecognitionSettings;
	remoteAvailable: boolean;
	onProviderChange: (provider: RecognitionProvider) => void;
	onLanguageChange: (language: RecognitionLanguage) => void;
	/** Present only when the server offers a verified wake release. */
	wakeAvailable?: boolean;
	wakeEnabled?: boolean;
	onWakeChange?: (enabled: boolean) => void;
	wakePhrases?: string[];
	wakePhrase?: string;
	onWakePhraseChange?: (phrase: string) => void;
	wakeState?: string;
	/** Input devices and the chosen one; null means the system default. */
	microphones?: MicrophoneDevice[];
	onMicrophoneChange?: (deviceId: string | null) => void;
	onWakeSilenceTimeoutChange?: (timeoutMs: number) => void;
	onWakeSoundChange?: (sound: WakeSound) => void;
	/** Debug view; offered only when a meter is supplied. */
	wakeMeter?: WakeMeter;
	onWakeDebugChange?: (enabled: boolean) => void;
	wakeReleaseThreshold?: number | null;
	onWakeThresholdChange?: (threshold: number | null) => void;
	disabled?: boolean;
}

/** Same controls before capture and in the dictation overlay; never switch silently. */
export function RecognitionControls({
	recognition,
	remoteAvailable,
	onProviderChange,
	onLanguageChange,
	wakeAvailable,
	wakeEnabled,
	onWakeChange,
	wakePhrases,
	wakePhrase,
	onWakePhraseChange,
	wakeState,
	microphones,
	onMicrophoneChange,
	onWakeSilenceTimeoutChange,
	onWakeSoundChange,
	wakeMeter,
	onWakeDebugChange,
	wakeReleaseThreshold,
	onWakeThresholdChange,
	disabled,
}: RecognitionControlsProps) {
	const id = useId();
	const { t } = useTranslation();
	const key = (name: string) => `voice.dictationSettings.${name}`;
	const selectClass =
		"w-full min-w-0 min-h-11 sm:min-h-0 rounded-md border border-input bg-background px-2 py-2 text-base sm:text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";
	return (
		<div className="space-y-2">
			<div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
				<label
					htmlFor={`${id}-provider`}
					className="min-w-0 space-y-1 text-xs text-muted-foreground"
				>
					<span className="flex items-center gap-1">
						{t(key("recognition"))}
						{/* A local provider keeps the control usable outside the app shell. */}
						<TooltipProvider delayDuration={150}>
							<Tooltip>
								<TooltipTrigger asChild>
									<button
										type="button"
										className="composer-dictation-settings__info"
										aria-label={t(key("recognitionHelp"))}
									>
										<Info aria-hidden="true" />
									</button>
								</TooltipTrigger>
								<TooltipContent
									side="top"
									className="max-w-72 space-y-2 text-xs"
								>
									<div>
										<strong>{t(key("providerLocalTitle"))}</strong>
										<div>{t(key("providerLocalSize"))}</div>
										<div>{t(key("providerLocalQuality"))}</div>
										<div>{t(key("providerLocalPrivacy"))}</div>
									</div>
									<div>
										<strong>{t(key("providerRemoteTitle"))}</strong>
										<div>{t(key("providerRemoteSize"))}</div>
										<div>{t(key("providerRemoteQuality"))}</div>
										<div>{t(key("providerRemotePrivacy"))}</div>
									</div>
								</TooltipContent>
							</Tooltip>
						</TooltipProvider>
					</span>
					<select
						id={`${id}-provider`}
						value={recognition.provider}
						disabled={disabled}
						className={selectClass}
						onChange={(event) =>
							onProviderChange(event.target.value as RecognitionProvider)
						}
					>
						<option value="moonshine">{t(key("providerLocal"))}</option>
						<option value="ears" disabled={!remoteAvailable}>
							{t(key("providerRemote"))}
							{!remoteAvailable ? t(key("providerNotConfigured")) : ""}
						</option>
					</select>
				</label>
				<label
					htmlFor={`${id}-language`}
					className="min-w-0 space-y-1 text-xs text-muted-foreground"
				>
					<span>{t(key("speechLanguage"))}</span>
					<select
						id={`${id}-language`}
						value={recognition.language}
						disabled={disabled}
						className={selectClass}
						onChange={(event) =>
							onLanguageChange(event.target.value as RecognitionLanguage)
						}
					>
						<option value="en">English</option>
						<option value="de">Deutsch</option>
					</select>
				</label>
			</div>
			<label
				htmlFor={`${id}-microphone`}
				className="block min-w-0 space-y-1 text-xs text-muted-foreground"
			>
				<span>{t(key("microphone"))}</span>
				<select
					id={`${id}-microphone`}
					value={recognition.microphoneId ?? ""}
					disabled={disabled}
					className={selectClass}
					onChange={(event) => onMicrophoneChange?.(event.target.value || null)}
				>
					<option value="">{t(key("microphoneDefault"))}</option>
					{(microphones ?? []).map((device, position) => (
						<option key={device.deviceId || position} value={device.deviceId}>
							{device.label || `${t(key("microphone"))} ${position + 1}`}
						</option>
					))}
				</select>
			</label>
			<p className="text-xs text-muted-foreground">
				{recognition.microphoneId === null ? t(key("microphoneHint")) : ""}
			</p>
			<p className="text-xs text-muted-foreground">
				{recognition.provider === "moonshine"
					? t(key("audioLocalNotice"))
					: t(key("audioRemoteNotice"))}
			</p>
			{wakeAvailable ? (
				<div className="space-y-1">
					<label className="flex items-center gap-2 text-xs text-muted-foreground">
						<input
							id={`${id}-wake`}
							type="checkbox"
							checked={wakeEnabled === true}
							disabled={disabled}
							onChange={(event) => onWakeChange?.(event.target.checked)}
						/>
						<span>{t(key("wakeToggle"))}</span>
					</label>
					{wakeEnabled && (wakePhrases?.length ?? 0) > 1 ? (
						<select
							aria-label={t(key("wakePhraseLabel"))}
							value={wakePhrase}
							disabled={disabled}
							className={selectClass}
							onChange={(event) => onWakePhraseChange?.(event.target.value)}
						>
							{wakePhrases?.map((phrase) => (
								<option key={phrase} value={phrase}>
									{phrase.replaceAll("_", " ")}
								</option>
							))}
						</select>
					) : null}
					<p className="text-xs text-muted-foreground">
						{wakeEnabled
							? `${t(key("wakeListening"))}${wakeState === "muted" ? t(key("wakeMutedSuffix")) : ""}. ${t(key("wakeListeningNote"))}`
							: t(key("wakeOffNote"))}
					</p>
					{wakeEnabled && onWakeSoundChange ? (
						<div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
							<label htmlFor={`${id}-wake-sound`}>{t(key("wakeSound"))}</label>
							<span className="flex items-center gap-1">
								<select
									id={`${id}-wake-sound`}
									className="min-h-8 rounded-md border border-input bg-background px-2 text-xs"
									value={recognition.wakeSound}
									onChange={(event) =>
										onWakeSoundChange(event.target.value as WakeSound)
									}
								>
									{WAKE_SOUNDS.map((sound) => (
										<option key={sound} value={sound}>
											{t(key(`wakeSound_${sound}`))}
										</option>
									))}
								</select>
								<button
									type="button"
									className="composer-dictation-settings__info"
									aria-label={t(key("wakeSoundPreview"))}
									title={t(key("wakeSoundPreview"))}
									disabled={recognition.wakeSound === "off"}
									onClick={() => playWakeSound(recognition.wakeSound)}
								>
									<Volume2 aria-hidden="true" />
								</button>
							</span>
						</div>
					) : null}
					{wakeEnabled && onWakeSilenceTimeoutChange ? (
						<label
							className="flex items-center justify-between gap-2 text-xs text-muted-foreground"
							title={t(key("wakeStopHint"))}
						>
							<span>{t(key("wakeStopAfter"))}</span>
							<select
								className="min-h-8 rounded-md border border-input bg-background px-2 text-xs"
								value={recognition.wakeSilenceTimeoutMs}
								onChange={(event) =>
									onWakeSilenceTimeoutChange(Number(event.target.value))
								}
							>
								{WAKE_SILENCE_TIMEOUTS_MS.map((timeout) => (
									<option key={timeout} value={timeout}>
										{timeout === 0
											? t(key("wakeStopNever"))
											: `${timeout / 1000} ${t(key("seconds"))}`}
									</option>
								))}
							</select>
						</label>
					) : null}
					{wakeEnabled && wakeMeter && onWakeDebugChange ? (
						<label className="flex items-center gap-2 text-xs text-muted-foreground">
							<input
								type="checkbox"
								checked={recognition.wakeDebug}
								onChange={(event) => onWakeDebugChange(event.target.checked)}
							/>
							<span>{t(key("wakeDebugToggle"))}</span>
						</label>
					) : null}
					{wakeEnabled && wakeMeter && recognition.wakeDebug ? (
						<WakeDebugPanel
							meter={wakeMeter}
							threshold={recognition.wakeThreshold}
							releaseThreshold={wakeReleaseThreshold ?? null}
							onThresholdChange={(value) => onWakeThresholdChange?.(value)}
							listening={wakeState === "listening"}
						/>
					) : null}
				</div>
			) : null}
		</div>
	);
}
