import type { RecognitionSettings } from "@/lib/voice/recognition-settings";
import type {
	RecognitionLanguage,
	RecognitionProvider,
} from "@byteowlz/ears-browser";
import { useId } from "react";

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
	disabled,
}: RecognitionControlsProps) {
	const id = useId();
	const selectClass =
		"w-full min-w-0 min-h-11 sm:min-h-0 rounded-md border border-input bg-background px-2 py-2 text-base sm:text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";
	return (
		<div className="space-y-2">
			<div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
				<label
					htmlFor={`${id}-provider`}
					className="min-w-0 space-y-1 text-xs text-muted-foreground"
				>
					<span>Recognition</span>
					<select
						id={`${id}-provider`}
						value={recognition.provider}
						disabled={disabled}
						className={selectClass}
						onChange={(event) =>
							onProviderChange(event.target.value as RecognitionProvider)
						}
					>
						<option value="moonshine">Local · Moonshine</option>
						<option value="ears" disabled={!remoteAvailable}>
							Remote · eaRS{!remoteAvailable ? " (not configured)" : ""}
						</option>
					</select>
				</label>
				<label
					htmlFor={`${id}-language`}
					className="min-w-0 space-y-1 text-xs text-muted-foreground"
				>
					<span>Speech language</span>
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
			<p className="text-xs text-muted-foreground">
				{recognition.provider === "moonshine"
					? "Audio stays on this device. First use downloads a speech model."
					: "Audio is sent to your configured eaRS service."}
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
						<span>Start dictation on a wake phrase</span>
					</label>
					{wakeEnabled && (wakePhrases?.length ?? 0) > 1 ? (
						<select
							aria-label="Wake phrase"
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
							? `Listening locally${wakeState === "muted" ? " · muted while speaking" : ""}. A heard phrase only starts dictation; it never sends or approves anything.`
							: "Off by default. Runs on this device; nothing is uploaded."}
					</p>
				</div>
			) : null}
		</div>
	);
}
