import type { WakeMeter } from "@/lib/voice/wake-meter";
import { useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";

export interface WakeDebugPanelProps {
	meter: WakeMeter;
	/** Local override, or null when the release threshold applies. */
	threshold: number | null;
	/** Threshold shipped with the release; null until it has loaded. */
	releaseThreshold: number | null;
	onThresholdChange: (threshold: number | null) => void;
	listening: boolean;
}

export const WAKE_THRESHOLD_MIN = 0.1;
export const WAKE_THRESHOLD_MAX = 0.95;

const fmt = (value: number) => value.toFixed(2);

/** Live wake score against the threshold, plus a local threshold slider (oqto-623a). */
export function WakeDebugPanel({
	meter,
	threshold,
	releaseThreshold,
	onThresholdChange,
	listening,
}: WakeDebugPanelProps) {
	const { t } = useTranslation();
	const key = (name: string) => `voice.dictationSettings.${name}`;
	const { score, peak, lastTrigger } = useSyncExternalStore(
		meter.subscribe,
		meter.get,
	);
	const effective = threshold ?? releaseThreshold;
	const above = effective !== null && score >= effective;
	const ago = lastTrigger
		? Math.max(0, Math.round((Date.now() - lastTrigger.at) / 1000))
		: null;
	return (
		<div className="space-y-1.5 rounded-md border border-border p-2 text-xs text-muted-foreground">
			<div
				role="meter"
				aria-label={t(key("wakeMeterLabel"))}
				aria-valuemin={0}
				aria-valuemax={1}
				aria-valuenow={Number(fmt(score))}
				className="relative h-2 overflow-hidden rounded bg-muted"
			>
				<div
					className={
						above ? "h-full bg-primary" : "h-full bg-muted-foreground/60"
					}
					style={{ width: `${score * 100}%` }}
				/>
				<div
					aria-hidden
					className="composer-wake-meter__peak absolute top-0 h-full w-px bg-muted-foreground"
					style={{ left: `${peak * 100}%` }}
				/>
				{threshold !== null && releaseThreshold !== null ? (
					<div
						aria-hidden
						title="Release threshold"
						className="absolute top-0 h-full w-px bg-foreground/40"
						style={{ left: `${releaseThreshold * 100}%` }}
					/>
				) : null}
				{effective !== null ? (
					<div
						aria-hidden
						className="absolute top-0 h-full w-0.5 bg-foreground"
						style={{ left: `${effective * 100}%` }}
					/>
				) : null}
			</div>
			<p className="tabular-nums">
				{listening
					? `${t(key("wakeScore"))} ${fmt(score)} · ${fmt(peak)}`
					: t(key("wakeNotListening"))}
				{lastTrigger && ago !== null
					? ` · ${t(key("wakeLastTrigger"))} ${fmt(lastTrigger.score)}`
					: ""}
			</p>
			<label className="block space-y-0.5">
				<span className="flex items-center justify-between gap-2">
					<span>
						{t(key("wakeThreshold"))}{" "}
						{effective !== null ? fmt(effective) : "…"}
						{threshold === null
							? ` (${t(key("wakeThresholdRelease"))})`
							: ` (${t(key("wakeThresholdCustom"))})`}
					</span>
					{threshold !== null ? (
						<button
							type="button"
							className="underline underline-offset-2"
							onClick={() => onThresholdChange(null)}
						>
							{t(key("wakeUseReleaseThreshold"))}
						</button>
					) : null}
				</span>
				<input
					type="range"
					aria-label={t(key("wakeSliderLabel"))}
					className="composer-wake-meter__slider w-full"
					min={WAKE_THRESHOLD_MIN}
					max={WAKE_THRESHOLD_MAX}
					step={0.01}
					value={effective ?? 0.5}
					disabled={effective === null}
					onChange={(event) => onThresholdChange(Number(event.target.value))}
				/>
			</label>
		</div>
	);
}
