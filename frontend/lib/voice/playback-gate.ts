/**
 * Half-duplex gate between app playback and wake listening (waqy).
 *
 * The speaker output reaches the microphone, so the detector must ignore audio
 * while the app speaks (and for a short echo tail after). TTS is owned by
 * different components in the legacy UI and OqtoUI, so this is a single
 * process-local signal instead of prop drilling. A detection is evidence only.
 */
type Listener = (active: boolean) => void;

const listeners = new Set<Listener>();
let active = false;

export function setAppPlayback(playing: boolean): void {
	if (active === playing) return;
	active = playing;
	for (const listener of [...listeners]) {
		try {
			listener(playing);
		} catch {
			// a gate listener must never break playback
		}
	}
}
export function isAppPlaying(): boolean {
	return active;
}
/** Returns an unsubscribe function; reports the current state immediately. */
export function onAppPlayback(listener: Listener): () => void {
	listeners.add(listener);
	listener(active);
	return () => listeners.delete(listener);
}
/** Tests only: forget listeners and state. */
export function resetPlaybackGateForTests(): void {
	listeners.clear();
	active = false;
}
