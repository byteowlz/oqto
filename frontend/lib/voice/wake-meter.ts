/**
 * Live wake score for the debug view (oqto-623a). Scores arrive every 80 ms, so
 * the store is subscribed to directly by the meter instead of living in React
 * state, which would re-render the whole composer at 12.5 Hz.
 */
export interface WakeMeterSnapshot {
	/** Latest score of the selected phrase, 0..1. */
	score: number;
	/** Recent maximum with a slow decay, so short peaks stay readable. */
	peak: number;
	/** Score and time of the last detection of the selected phrase. */
	lastTrigger: { score: number; at: number } | null;
}

const EMPTY: WakeMeterSnapshot = { score: 0, peak: 0, lastTrigger: null };
/** Peak decay per frame: a full-scale peak fades in about two seconds. */
const PEAK_DECAY = 0.04;
/**
 * Scores arrive every 80 ms. Notifying subscribers that often makes the bar
 * visibly flash, so updates are coalesced to ~10 Hz with a trailing edge: the
 * last value always arrives, and nothing is dropped that the user could read.
 */
const MIN_NOTIFY_MS = 100;

export class WakeMeter {
	private snapshot: WakeMeterSnapshot = EMPTY;
	private readonly listeners = new Set<() => void>();
	private lastNotify = 0;
	private trailing: ReturnType<typeof setTimeout> | null = null;

	subscribe = (listener: () => void): (() => void) => {
		this.listeners.add(listener);
		return () => this.listeners.delete(listener);
	};

	get = (): WakeMeterSnapshot => this.snapshot;

	push(score: number): void {
		const clamped = Math.min(1, Math.max(0, score));
		this.set({
			...this.snapshot,
			score: clamped,
			peak: Math.max(clamped, this.snapshot.peak - PEAK_DECAY),
		});
	}

	trigger(score: number, at = Date.now()): void {
		// A detection is a discrete event: show it immediately, never coalesced.
		this.snapshot = { ...this.snapshot, lastTrigger: { score, at } };
		if (this.trailing) {
			clearTimeout(this.trailing);
			this.trailing = null;
		}
		this.emit();
	}

	/** Listening stopped: drop the live level, keep the last trigger for reading. */
	idle(): void {
		if (this.snapshot.score === 0 && this.snapshot.peak === 0) return;
		this.set({ ...this.snapshot, score: 0, peak: 0 });
	}

	private set(next: WakeMeterSnapshot) {
		this.snapshot = next;
		const now = Date.now();
		const wait = MIN_NOTIFY_MS - (now - this.lastNotify);
		if (wait <= 0) {
			this.emit();
			return;
		}
		this.trailing ??= setTimeout(() => {
			this.trailing = null;
			this.emit();
		}, wait);
	}
	private emit() {
		this.lastNotify = Date.now();
		for (const listener of this.listeners) listener();
	}
	/** Tests and teardown: drop any pending coalesced notification. */
	dispose() {
		if (this.trailing) clearTimeout(this.trailing);
		this.trailing = null;
		this.listeners.clear();
	}
}
