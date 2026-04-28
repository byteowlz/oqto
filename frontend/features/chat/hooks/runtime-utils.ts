export const BATCH_FLUSH_INTERVAL_MS = 50;

// Streaming delta coalescing interval. During fast streaming, text_delta and
// thinking_delta events arrive much faster than React can usefully re-render.
// We coalesce intermediate accumulated snapshots and emit at this cadence.
// Inspired by pi-mobile's UiUpdateThrottler. We use a slightly higher
// cadence to reduce visible layout twitch during very fast token bursts.
export const TEXT_DELTA_THROTTLE_MS = 100;

export function isPiDebugEnabled(): boolean {
	if (!import.meta.env.DEV) return false;
	try {
		if (typeof localStorage !== "undefined") {
			return localStorage.getItem("debug:pi-v2") === "1";
		}
	} catch {
		// ignore
	}
	return import.meta.env.VITE_DEBUG_PI_V2 === "1";
}

export function createTempMessageId(): string {
	if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
		return `tmp:${crypto.randomUUID()}`;
	}
	return `tmp:${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
