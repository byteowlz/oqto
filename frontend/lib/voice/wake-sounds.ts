/**
 * Short confirmation sounds for a detected wake phrase (oqto-623a).
 *
 * Synthesized with WebAudio, so there are no audio assets, licences or
 * downloads. Each sound is under ~400 ms and quiet: it confirms "I heard you"
 * without masking the first words of the dictation that starts right after.
 */
export const WAKE_SOUNDS = ["off", "chime", "bell", "pop", "soft"] as const;
export type WakeSound = (typeof WAKE_SOUNDS)[number];
export const DEFAULT_WAKE_SOUND: WakeSound = "chime";

export function isWakeSound(value: unknown): value is WakeSound {
	return (WAKE_SOUNDS as readonly unknown[]).includes(value);
}

/** One partial: a tone that starts at `at` seconds and decays over `decay`. */
interface Note {
	at: number;
	frequency: number;
	decay: number;
	gain: number;
	type: OscillatorType;
	/** Optional pitch glide target, for the "pop". */
	glideTo?: number;
}

const PRESETS: Record<Exclude<WakeSound, "off">, Note[]> = {
	// Two rising notes (E5 -> B5): the classic "ready" cue.
	chime: [
		{ at: 0, frequency: 659.25, decay: 0.18, gain: 0.16, type: "sine" },
		{ at: 0.09, frequency: 987.77, decay: 0.28, gain: 0.14, type: "sine" },
	],
	// A single bell strike with a few inharmonic partials.
	bell: [
		{ at: 0, frequency: 880, decay: 0.38, gain: 0.12, type: "sine" },
		{ at: 0, frequency: 2398, decay: 0.16, gain: 0.04, type: "sine" },
		{ at: 0, frequency: 3520, decay: 0.08, gain: 0.025, type: "sine" },
	],
	// A short upward blip.
	pop: [
		{
			at: 0,
			frequency: 420,
			glideTo: 980,
			decay: 0.09,
			gain: 0.18,
			type: "sine",
		},
	],
	// Low, gentle two-note triangle for quiet rooms.
	soft: [
		{ at: 0, frequency: 523.25, decay: 0.22, gain: 0.08, type: "triangle" },
		{ at: 0.11, frequency: 659.25, decay: 0.3, gain: 0.07, type: "triangle" },
	],
};

/** The parts of AudioContext this module uses (injectable for tests). */
export interface ToneContext {
	readonly currentTime: number;
	readonly state: AudioContextState;
	readonly destination: AudioNode;
	resume(): Promise<void>;
	createOscillator(): OscillatorNode;
	createGain(): GainNode;
}

let shared: ToneContext | null = null;
let factory: () => ToneContext | null = () => {
	const Ctor =
		globalThis.AudioContext ??
		(globalThis as { webkitAudioContext?: typeof AudioContext })
			.webkitAudioContext;
	return Ctor ? new Ctor() : null;
};

function context(): ToneContext | null {
	shared ??= factory();
	return shared;
}

/**
 * Browsers only allow audio after a user gesture. Wake can be armed straight
 * from a reload, so the context is unlocked on the first pointer or key
 * gesture anywhere on the page. Without one, the first wake stays silent.
 */
export function unlockWakeSoundsOnGesture(
	target: EventTarget = globalThis,
): () => void {
	const unlock = () => {
		void context()
			?.resume()
			.catch(() => undefined);
		target.removeEventListener("pointerdown", unlock);
		target.removeEventListener("keydown", unlock);
	};
	target.addEventListener("pointerdown", unlock, { passive: true });
	target.addEventListener("keydown", unlock);
	return unlock;
}

/** Play a wake sound. Never throws and never blocks: sound is decoration. */
export function playWakeSound(sound: WakeSound): boolean {
	if (sound === "off") return false;
	const ctx = context();
	if (!ctx) return false;
	try {
		if (ctx.state === "suspended") void ctx.resume().catch(() => undefined);
		const start = ctx.currentTime + 0.005;
		for (const note of PRESETS[sound]) {
			const osc = ctx.createOscillator();
			const env = ctx.createGain();
			const t0 = start + note.at;
			osc.type = note.type;
			osc.frequency.setValueAtTime(note.frequency, t0);
			if (note.glideTo)
				osc.frequency.exponentialRampToValueAtTime(
					note.glideTo,
					t0 + note.decay,
				);
			// Fast attack, exponential decay: no clicks at either end.
			env.gain.setValueAtTime(0.0001, t0);
			env.gain.exponentialRampToValueAtTime(note.gain, t0 + 0.008);
			env.gain.exponentialRampToValueAtTime(0.0001, t0 + note.decay);
			osc.connect(env);
			env.connect(ctx.destination);
			osc.start(t0);
			osc.stop(t0 + note.decay + 0.02);
		}
		return true;
	} catch {
		return false;
	}
}

export function setToneContextFactoryForTests(
	make: () => ToneContext | null,
): void {
	shared = null;
	factory = make;
}
