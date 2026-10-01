import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import type { UseDictationReturn } from "@/features/voice/hooks/useDictation";
import { cn } from "@/lib/utils";
import { AudioLines, Check, Mic, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
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
	const { i18n } = useTranslation();
	const de = i18n.resolvedLanguage?.startsWith("de");
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
						? de
							? "Lokal"
							: "Local"
						: "eaRS"}
					{dictation.autoSendEnabled ? " · Auto" : ""}
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

/** Settings are available before recording, not repeated inside the status strip. */
export function DictationMicButton({
	dictation,
	className,
}: { dictation: UseDictationReturn; className?: string }) {
	const [open, setOpen] = useState(false);
	const { i18n } = useTranslation();
	const de = i18n.resolvedLanguage?.startsWith("de");
	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				<button
					type="button"
					className={cn(
						"composer-dictation-mic",
						dictation.isActive && "composer-dictation-mic--active",
						className,
					)}
					aria-label={de ? "Diktiereinstellungen" : "Dictation settings"}
					aria-pressed={dictation.isActive}
				>
					{dictation.isActive ? (
						<AudioLines aria-hidden="true" />
					) : (
						<Mic aria-hidden="true" />
					)}
				</button>
			</PopoverTrigger>
			<PopoverContent
				side="top"
				align="start"
				className="composer-dictation-settings"
			>
				<RecognitionControls
					recognition={dictation.recognition}
					remoteAvailable={dictation.remoteAvailable}
					onProviderChange={dictation.setRecognitionProvider}
					onLanguageChange={dictation.setRecognitionLanguage}
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
					{de
						? "Fertige Sätze automatisch senden"
						: "Auto-send completed speech"}
				</label>
				<button
					type="button"
					className="composer-dictation-settings__start"
					disabled={dictation.isActive}
					onClick={() => {
						setOpen(false);
						void dictation.start();
					}}
				>
					{de ? "Diktieren starten" : "Start dictation"}
				</button>
			</PopoverContent>
		</Popover>
	);
}
