import { cn } from "@/lib/utils";
import { useEffect, useRef, useState } from "react";

/**
 * The Oqtopus mascot as the voice indicator (oqto-623a).
 *
 * The oqto-animation vector player is used unchanged through its documented
 * embedding contract: a same-origin host page (staged by
 * scripts/sync-oqtopus-assets.mjs into public/oqtopus/) that publishes
 * `window.oqtopus`. Oqto only reports what happened; the player decides how
 * it moves. Without staged assets nothing is rendered and callers keep their
 * plain icon.
 */
const BASE = `${import.meta.env.BASE_URL}oqtopus/`;

/** Event kinds the voice surface reports, mapped to library actions. */
export const OQTOPUS_REACTIONS = {
	wake: "wave",
	sent: "hat-tip",
	error: "surprised",
} as const;
export type OqtopusCue = {
	kind: keyof typeof OQTOPUS_REACTIONS;
	at: number;
};
/** A reaction that arrives later than this (slow first load) is skipped. */
const CUE_FRESH_MS = 4000;

interface OqtopusApi {
	play(action: string): void;
	setIdle(on: boolean): void;
	setTheme(theme: "auto" | "dark" | "light"): void;
}

let availability: Promise<boolean> | null = null;
/** Whether the mascot was staged into this build; checked once per page. */
export function oqtopusAvailable(): Promise<boolean> {
	// Any failure (no network, missing file, unusual fetch) means "not staged".
	availability ??= Promise.resolve()
		.then(() => fetch(`${BASE}library.json`, { method: "HEAD" }))
		.then((response) => Boolean(response?.ok))
		.catch(() => false);
	return availability;
}
export function resetOqtopusAvailabilityForTests(): void {
	availability = null;
}

/** True once the staged mascot is known to exist; false while unknown or absent. */
export function useOqtopusAvailable(wanted: boolean): boolean {
	const [available, setAvailable] = useState(false);
	// useeffect-guardrail: allow — one cached availability probe for an external static asset.
	useEffect(() => {
		if (!wanted) return;
		let live = true;
		void oqtopusAvailable().then((ok) => {
			if (live) setAvailable(ok);
		});
		return () => {
			live = false;
		};
	}, [wanted]);
	return wanted && available;
}

export function OqtopusMascot({
	lively,
	cue,
	theme,
	className,
}: {
	/** Rotating idles (dictating) instead of breathing at rest (waiting). */
	lively: boolean;
	cue: OqtopusCue | null;
	theme: "dark" | "light";
	className?: string;
}) {
	const frame = useRef<HTMLIFrameElement>(null);
	const played = useRef<number | null>(null);
	const desired = useRef({ lively, cue, theme });
	desired.current = { lively, cue, theme };

	// useeffect-guardrail: allow — drives an embedded player that publishes its API asynchronously.
	useEffect(() => {
		const apply = (want = desired.current): boolean => {
			const api = (
				frame.current?.contentWindow as
					| (Window & { oqtopus?: OqtopusApi })
					| null
			)?.oqtopus;
			if (!api) return false;
			try {
				api.setTheme(want.theme);
				api.setIdle(want.lively);
				if (want.cue && want.cue.at !== played.current) {
					played.current = want.cue.at;
					if (Date.now() - want.cue.at <= CUE_FRESH_MS)
						api.play(OQTOPUS_REACTIONS[want.cue.kind]);
				}
			} catch {
				// A player error must never break the composer.
			}
			return true;
		};
		if (apply({ lively, cue, theme })) return;
		// The player loads its vectors first, then publishes window.oqtopus.
		const poll = setInterval(() => {
			if (apply()) clearInterval(poll);
		}, 150);
		const giveUp = setTimeout(() => clearInterval(poll), 15_000);
		return () => {
			clearInterval(poll);
			clearTimeout(giveUp);
		};
	}, [lively, cue, theme]);

	return (
		<iframe
			ref={frame}
			title="Oqtopus"
			aria-hidden="true"
			tabIndex={-1}
			src={`${BASE}embed.html?rest=normal&breath=soft&idle=0&theme=${theme}`}
			className={cn("composer-dictation-mic__oqtopus", className)}
			// A colour scheme matching the embedded page keeps the iframe transparent.
			style={{ colorScheme: theme }}
		/>
	);
}
