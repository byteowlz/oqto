export type HoldIntentTimer = ReturnType<typeof setTimeout>;

export function scheduleHoldIntent(
	holdMs: number,
	onHold: () => void,
): HoldIntentTimer {
	return setTimeout(onHold, holdMs);
}

export function cancelHoldIntent(timer: HoldIntentTimer | null): void {
	if (timer) clearTimeout(timer);
}
