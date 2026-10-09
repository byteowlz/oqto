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
	disabled?: boolean;
}

/** Same controls before capture and in the dictation overlay; never switch silently. */
export function RecognitionControls({
	recognition,
	remoteAvailable,
	onProviderChange,
	onLanguageChange,
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
		</div>
	);
}
