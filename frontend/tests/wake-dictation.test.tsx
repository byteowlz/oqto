import { useDictation } from "@/features/voice/hooks/useDictation";
import { RECOGNITION_SETTINGS_KEY } from "@/lib/voice/recognition-settings";
import type { STTCallbacks } from "@/lib/voice/stt-service";
import type {
	WakeCallbacks,
	WakeDeps,
	WakeReleaseLocation,
} from "@/lib/voice/wake-service";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

type Frame = { scores: Record<string, number>; detections: string[] };
const h = vi.hoisted(() => ({
	frames: [] as Frame[],
	feed: undefined as ((pcm: Float32Array) => void) | undefined,
	createEngine: undefined as unknown as ReturnType<typeof vi.fn>,
	loadManifest: undefined as unknown as ReturnType<typeof vi.fn>,
	sttStarts: 0,
	stt: [] as { callbacks: STTCallbacks }[],
}));

// Real WakeService; only the release download, engine and microphone are faked.
vi.mock("@/lib/voice/wake-service", async (importOriginal) => {
	const actual =
		await importOriginal<typeof import("@/lib/voice/wake-service")>();
	h.loadManifest = vi.fn(async () => ({
		release: "fixture",
		threshold: 0.5,
		classifiers: ["hey_oqto"],
		files: {
			mel: { sha256: "a" },
			embedding: { sha256: "a" },
			hey_oqto: { sha256: "a" },
		},
	}));
	h.createEngine = vi.fn(async () => ({
		push: vi.fn(async () => h.frames.splice(0)),
		setPlayback: vi.fn(),
		reset: vi.fn(),
	}));
	const deps: WakeDeps = {
		loadManifest: h.loadManifest,
		loadFile: vi.fn(async () => new Uint8Array([1])),
		createEngine: h.createEngine,
		openCapture: vi.fn(async (_rate, push) => {
			h.feed = push;
			return { close: vi.fn(async () => undefined), setEnabled: vi.fn() };
		}),
	} as unknown as WakeDeps;
	class FixtureWakeService extends actual.WakeService {
		constructor(location: WakeReleaseLocation, callbacks: WakeCallbacks) {
			super(location, callbacks, deps);
		}
	}
	return { ...actual, WakeService: FixtureWakeService };
});

vi.mock("@/lib/voice/stt-service", () => ({
	STTService: vi.fn().mockImplementation(() => {
		const service = {
			callbacks: {} as STTCallbacks,
			setCallbacks(callbacks: STTCallbacks) {
				service.callbacks = callbacks;
			},
			async startListening() {
				h.sttStarts++;
			},
			async finishListening() {},
			disconnect() {},
			getInputVolume: () => 0,
		};
		h.stt.push(service);
		return service;
	}),
}));
const sounds = vi.hoisted(() => ({ played: [] as string[] }));
vi.mock("@/lib/voice/wake-sounds", async (importOriginal) => {
	const actual =
		await importOriginal<typeof import("@/lib/voice/wake-sounds")>();
	return {
		...actual,
		playWakeSound: (sound: string) => {
			sounds.played.push(sound);
			return sound !== "off";
		},
		unlockWakeSoundsOnGesture: () => () => {},
	};
});
vi.mock("@/lib/control-plane-client", () => ({
	voiceProxyWsUrl: vi.fn(() => "wss://example.test/voice/stt"),
}));

beforeEach(() => {
	sounds.played.length = 0;
	h.frames.length = 0;
	h.sttStarts = 0;
	h.stt.length = 0;
	const storage = new Map<string, string>([
		[
			RECOGNITION_SETTINGS_KEY,
			JSON.stringify({ wakeEnabled: true, wakeDebug: true }),
		],
	]);
	vi.mocked(localStorage.getItem).mockImplementation(
		(key) => storage.get(key) ?? null,
	);
	vi.mocked(localStorage.setItem).mockImplementation((key, value) => {
		storage.set(key, value);
	});
});

function hook() {
	return renderHook(() =>
		useDictation({
			scopeKey: "session",
			config: null,
			onTranscript: vi.fn(),
			wakeRelease: { manifest_url: "/m", files_url: "/f" },
		}),
	);
}

async function hear(score: number) {
	h.frames.push({
		scores: { hey_oqto: score },
		detections: score >= 0.5 ? ["hey_oqto"] : [],
	});
	await act(async () => {
		h.feed?.(new Float32Array(1280));
		await new Promise((resolve) => setTimeout(resolve, 0));
	});
}

describe("wake threshold through the dictation hook", () => {
	it("a score below the release threshold starts dictation only after the slider lowers it", async () => {
		const { result } = hook();
		await waitFor(() => expect(result.current.wakeState).toBe("listening"));
		expect(result.current.wakeReleaseThreshold).toBe(0.5);

		await hear(0.42);
		expect(result.current.wakeMeter.get().score).toBe(0.42);
		expect(result.current.isActive).toBe(false);

		act(() => result.current.setWakeThreshold(0.4));
		await hear(0.42);
		await waitFor(() => expect(result.current.isActive).toBe(true));
		expect(result.current.wakeTriggerScore).toBe(0.42);
		expect(result.current.wakeMeter.get().lastTrigger?.score).toBe(0.42);
	});

	it("a raised threshold ignores a score the release would have accepted", async () => {
		const { result } = hook();
		await waitFor(() => expect(result.current.wakeState).toBe("listening"));
		act(() => result.current.setWakeThreshold(0.7));
		await hear(0.6);
		expect(result.current.isActive).toBe(false);
		await hear(0.75);
		await waitFor(() => expect(result.current.isActive).toBe(true));
	});

	it("keeps the verified wake engine across a wake-triggered dictation", async () => {
		const { result } = hook();
		await waitFor(() => expect(result.current.wakeState).toBe("listening"));
		const engines = h.createEngine.mock.calls.length;
		const manifests = h.loadManifest.mock.calls.length;
		await hear(0.9);
		await waitFor(() => expect(result.current.isActive).toBe(true));
		await waitFor(() => expect(result.current.wakeState).toBe("off"));
		act(() => result.current.cancel());
		await waitFor(() => expect(result.current.wakeState).toBe("listening"));
		expect(h.createEngine.mock.calls.length).toBe(engines);
		expect(h.loadManifest.mock.calls.length).toBe(manifests);
	});

	it("plays the chosen sound on a detection, and none when it is off", async () => {
		const { result } = hook();
		await waitFor(() => expect(result.current.wakeState).toBe("listening"));
		await hear(0.9);
		await waitFor(() => expect(result.current.isActive).toBe(true));
		expect(sounds.played).toEqual(["chime"]);
		expect(result.current.voiceCue?.kind).toBe("wake");
		act(() => result.current.cancel());
		await waitFor(() => expect(result.current.wakeState).toBe("listening"));
		act(() => result.current.setWakeSound("off"));
		// Choosing a sound previews it; "off" previews nothing audible.
		expect(sounds.played.at(-1)).toBe("off");
		sounds.played.length = 0;
		// The wake service ignores a repeat inside its 2 s refractory window.
		const now = Date.now();
		const clock = vi.spyOn(Date, "now").mockReturnValue(now + 3_000);
		await hear(0.9);
		await waitFor(() => expect(result.current.isActive).toBe(true));
		expect(sounds.played).toEqual(["off"]);
		clock.mockRestore();
	});
});
