import { parseRecognitionSettings } from "@/lib/voice/recognition-settings";
import {
	DEFAULT_WAKE_SOUND,
	WAKE_SOUNDS,
	isWakeSound,
	playWakeSound,
	setToneContextFactoryForTests,
	unlockWakeSoundsOnGesture,
} from "@/lib/voice/wake-sounds";
import { afterEach, describe, expect, it, vi } from "vitest";

function fakeContext(state: AudioContextState = "running") {
	const started: { frequency: number; at: number; stop: number }[] = [];
	const ctx = {
		currentTime: 1,
		state,
		destination: {} as AudioNode,
		resume: vi.fn(async () => undefined),
		createGain: () =>
			({
				gain: {
					setValueAtTime: vi.fn(),
					exponentialRampToValueAtTime: vi.fn(),
				},
				connect: vi.fn(),
			}) as unknown as GainNode,
		createOscillator: () => {
			const node = {
				type: "sine",
				frequency: {
					value: 0,
					setValueAtTime: vi.fn((value: number) => {
						node.frequency.value = value;
					}),
					exponentialRampToValueAtTime: vi.fn(),
				},
				connect: vi.fn(),
				start: vi.fn((at: number) => {
					node.at = at;
				}),
				stop: vi.fn((at: number) => {
					started.push({
						frequency: node.frequency.value,
						at: node.at,
						stop: at,
					});
				}),
				at: 0,
			};
			return node as unknown as OscillatorNode;
		},
	};
	return { ctx, started };
}

afterEach(() => setToneContextFactoryForTests(() => null));

describe("wake sounds", () => {
	it("plays every sound except 'off', briefly", () => {
		for (const sound of WAKE_SOUNDS) {
			const { ctx, started } = fakeContext();
			setToneContextFactoryForTests(() => ctx);
			const played = playWakeSound(sound);
			expect(played, sound).toBe(sound !== "off");
			if (sound === "off") {
				expect(started).toEqual([]);
				continue;
			}
			expect(started.length, sound).toBeGreaterThan(0);
			// Short enough not to mask the first words of the dictation.
			const longest = Math.max(...started.map((n) => n.stop - ctx.currentTime));
			expect(longest, sound).toBeLessThan(0.45);
		}
	});

	it("resumes a suspended context instead of failing", () => {
		const { ctx } = fakeContext("suspended");
		setToneContextFactoryForTests(() => ctx);
		expect(playWakeSound("chime")).toBe(true);
		expect(ctx.resume).toHaveBeenCalled();
	});

	it("is silent, not broken, without WebAudio", () => {
		setToneContextFactoryForTests(() => null);
		expect(playWakeSound("chime")).toBe(false);
	});

	it("unlocks audio on the first gesture and only once", () => {
		const { ctx } = fakeContext("suspended");
		setToneContextFactoryForTests(() => ctx);
		const target = new EventTarget();
		unlockWakeSoundsOnGesture(target);
		target.dispatchEvent(new Event("pointerdown"));
		target.dispatchEvent(new Event("keydown"));
		expect(ctx.resume).toHaveBeenCalledTimes(1);
	});

	it("persists only known sounds and defaults to a chime", () => {
		expect(DEFAULT_WAKE_SOUND).toBe("chime");
		expect(isWakeSound("bell")).toBe(true);
		expect(isWakeSound("airhorn")).toBe(false);
		expect(parseRecognitionSettings('{"wakeSound":"pop"}').wakeSound).toBe(
			"pop",
		);
		expect(parseRecognitionSettings('{"wakeSound":"off"}').wakeSound).toBe(
			"off",
		);
		expect(parseRecognitionSettings('{"wakeSound":"airhorn"}').wakeSound).toBe(
			"chime",
		);
		expect(parseRecognitionSettings("{}").wakeSound).toBe("chime");
	});
});
