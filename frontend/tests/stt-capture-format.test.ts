import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({ feed: vi.fn() }));
vi.mock("@byteowlz/ears-browser", () => ({
	Recognizer: class {
		subscribe() {}
		async prepare() {}
		feed(audio: Float32Array) {
			state.feed(audio);
		}
		cancel() {}
		async finish() {}
	},
}));
import { STTService } from "@/lib/voice/stt-service";

let port: { onmessage?: (event: { data: Float32Array }) => void };
let addModule: ReturnType<typeof vi.fn>;
let stop: ReturnType<typeof vi.fn>;
beforeEach(() => {
	state.feed.mockReset();
	addModule = vi.fn().mockResolvedValue(undefined);
	stop = vi.fn();
	vi.stubGlobal("isSecureContext", true);
	vi.stubGlobal("navigator", {
		mediaDevices: {
			getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [{ stop }] }),
		},
	});
	const node = () => ({ connect: vi.fn(), disconnect: vi.fn(), fftSize: 0 });
	vi.stubGlobal(
		"AudioContext",
		class {
			sampleRate = 24000;
			state = "running";
			destination = {};
			audioWorklet = { addModule };
			createMediaStreamSource() {
				return node();
			}
			createAnalyser() {
				return node();
			}
			async resume() {}
			async close() {
				this.state = "closed";
			}
		},
	);
	vi.stubGlobal(
		"AudioWorkletNode",
		class {
			port = {};
			constructor() {
				port = this.port;
			}
			connect() {}
			disconnect() {}
		},
	);
});
afterEach(() => vi.unstubAllGlobals());

async function captured(samples: number[]): Promise<STTService> {
	const service = new STTService("", 1500, {
		provider: "moonshine",
		language: "en",
	});
	await service.startListening();
	for (let index = 0; index < 8; index++)
		port.onmessage?.({ data: Float32Array.from(samples) });
	return service;
}

describe("shared microphone PCM boundary", () => {
	it("clips finite WebAudio overshoot before the normalized-PCM recognizer contract", async () => {
		const service = await captured([
			0, 0.5, 1.0000076293945312, -1.0000076293945312,
		]);
		try {
			expect(state.feed).toHaveBeenCalledTimes(1);
			const audio = state.feed.mock.calls[0][0] as Float32Array;
			expect(Array.from(audio.slice(0, 4))).toEqual([0, 0.5, 1, -1]);
			expect(
				audio.every((value) => Number.isFinite(value) && Math.abs(value) <= 1),
			).toBe(true);
			expect(addModule).toHaveBeenCalledTimes(1);
			const url = addModule.mock.calls[0][0] as string;
			expect(url).toContain("capture-worklet");
			expect(url.startsWith("blob:") || url.startsWith("data:")).toBe(false);
		} finally {
			service.disconnect();
		}
		expect(stop).toHaveBeenCalled();
	});

	it("does not hide non-finite capture errors from the recognizer", async () => {
		const service = await captured([Number.NaN, Number.POSITIVE_INFINITY]);
		try {
			const audio = state.feed.mock.calls[0][0] as Float32Array;
			expect(Number.isNaN(audio[0])).toBe(true);
			expect(audio[1]).toBe(Number.POSITIVE_INFINITY);
		} finally {
			service.disconnect();
		}
	});
});
