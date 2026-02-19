/**
 * Streaming update throttle with coalescing.
 *
 * Inspired by pi-mobile's UiUpdateThrottler pattern. During fast streaming
 * (text_delta, thinking_delta), individual deltas arrive much faster than
 * React can usefully re-render. This throttle coalesces intermediate values
 * and emits at a fixed cadence, ensuring:
 *
 * 1. The first update after a pause is emitted immediately (responsive feel)
 * 2. Intermediate updates are coalesced (reduces render pressure)
 * 3. A flush ensures no pending data is lost on stream end
 *
 * Usage:
 *   const throttle = createStreamingThrottle<string>(80);
 *   // On each delta:
 *   const immediate = throttle.offer(accumulatedText);
 *   if (immediate !== null) applyToUI(immediate);
 *   // On timer tick (setInterval at minIntervalMs):
 *   const ready = throttle.drainReady();
 *   if (ready !== null) applyToUI(ready);
 *   // On stream end:
 *   const final = throttle.flush();
 *   if (final !== null) applyToUI(final);
 */

export interface StreamingThrottle<T> {
	/**
	 * Offer a new value. Returns the value immediately if enough time
	 * has elapsed since the last emission, otherwise stores it as pending
	 * and returns null.
	 */
	offer(value: T): T | null;

	/**
	 * Check if a pending value is ready to emit (enough time elapsed).
	 * Returns the pending value or null.
	 */
	drainReady(): T | null;

	/**
	 * Force-flush any pending value regardless of timing.
	 * Returns the pending value or null.
	 */
	flush(): T | null;

	/** Whether there is a pending (coalesced) value. */
	hasPending(): boolean;

	/** Reset all state. */
	reset(): void;
}

export function createStreamingThrottle<T>(
	minIntervalMs: number,
): StreamingThrottle<T> {
	let lastEmissionAt: number | null = null;
	let pending: T | null = null;
	let hasPendingValue = false;

	function canEmitNow(): boolean {
		if (lastEmissionAt === null) return true;
		return Date.now() - lastEmissionAt >= minIntervalMs;
	}

	function recordEmission(): void {
		lastEmissionAt = Date.now();
	}

	return {
		offer(value: T): T | null {
			if (canEmitNow()) {
				recordEmission();
				pending = null;
				hasPendingValue = false;
				return value;
			}
			pending = value;
			hasPendingValue = true;
			return null;
		},

		drainReady(): T | null {
			if (!hasPendingValue || !canEmitNow()) {
				return null;
			}
			const value = pending;
			pending = null;
			hasPendingValue = false;
			recordEmission();
			return value;
		},

		flush(): T | null {
			if (!hasPendingValue) return null;
			const value = pending;
			pending = null;
			hasPendingValue = false;
			recordEmission();
			return value;
		},

		hasPending(): boolean {
			return hasPendingValue;
		},

		reset(): void {
			pending = null;
			hasPendingValue = false;
			lastEmissionAt = null;
		},
	};
}
