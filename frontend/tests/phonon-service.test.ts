import type { PhononModelAssets } from "@/lib/voice/phonon-model";
import { PhononReadAloudService } from "@/lib/voice/phonon-service";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

class Source {
	buffer: AudioBuffer | null = null;
	playbackRate = { value: 1 };
	onended: (() => void) | null = null;
	connect = vi.fn();
	disconnect = vi.fn();
	start = vi.fn();
	stop = vi.fn();
	end() {
		this.onended?.();
	}
}
const sources: Source[] = [];
const close = vi.fn(async () => {});
const contextCreated = vi.fn();
class Context {
	constructor() {
		contextCreated();
	}
	currentTime = 0;
	destination = {};
	resume = vi.fn(async () => {});
	close = close;
	createBuffer(_channels: number, length: number, rate: number) {
		return { duration: length / rate, copyToChannel: vi.fn() };
	}
	createBufferSource() {
		const source = new Source();
		sources.push(source);
		return source;
	}
}
class TestWorker {
	onmessage: ((event: { data: unknown }) => void) | null = null;
	onerror: ((event: ErrorEvent) => void) | null = null;
	onmessageerror: (() => void) | null = null;
	postMessage = vi.fn();
	terminate = vi.fn();
	reply(type: string, value?: unknown, id?: number) {
		const last = this.postMessage.mock.calls.at(-1)?.[0];
		this.onmessage?.({ data: { type, id: id ?? last.id, value } });
	}
	chunk(pcm = new Float32Array([0.1, -0.1])) {
		const id = this.postMessage.mock.calls.at(-1)?.[0].id;
		this.onmessage?.({ data: { type: "chunk", id, pcm } });
	}
}
const ready = {
	sampleRate: 24000,
	languages: ["en"],
	device: "cpu",
	threads: 1,
};
const assets = (): PhononModelAssets => ({
	files: {
		"model.q8.gguf": "blob:weights",
		"config.json": "blob:config",
		"tokenizer.json": "blob:tokenizer",
		"default-voice.safetensors": "blob:voice",
	},
	release: vi.fn(),
});
const tick = async () => {
	for (let i = 0; i < 8; i++) await Promise.resolve();
};
function setup(patch = {}) {
	const worker = new TestWorker();
	const model = assets();
	const loadModel = vi.fn(async () => model);
	const onPlaying = vi.fn();
	const createWorker = vi.fn((_url: URL) => worker as unknown as Worker);
	const service = new PhononReadAloudService(
		{ voice: "default", speed: 1, language: "en", ...patch },
		{ onPlaying },
		{
			loadModel,
			createWorker,
			loadWasm: async () => ({
				wasmUrl: new ArrayBuffer(1),
				simdWasmUrl: new ArrayBuffer(1),
			}),
		},
	);
	return { service, worker, model, loadModel, createWorker, onPlaying };
}
async function prepared(patch = {}) {
	const fixture = setup(patch);
	const preparing = fixture.service.prepare();
	await tick();
	fixture.worker.reply("result", ready);
	await preparing;
	return fixture;
}
beforeEach(() => {
	sources.length = 0;
	vi.clearAllMocks();
	vi.stubGlobal("AudioContext", Context);
	vi.stubGlobal("Worker", TestWorker);
	vi.stubGlobal("isSecureContext", true);
	vi.stubGlobal(
		"WebSocket",
		vi.fn(() => {
			throw new Error("Local TTS must not open WebSockets");
		}),
	);
});
afterEach(() => {
	vi.unstubAllGlobals();
});

describe("pinned Phonon worker adapter", () => {
	it("memoizes concurrent and post-ready preparation without duplicating model, worker or AudioContext", async () => {
		const { service, worker, loadModel, createWorker } = setup();
		const first = service.prepare();
		const concurrent = service.prepare();
		expect(concurrent).toBe(first);
		await tick();
		expect(loadModel).toHaveBeenCalledOnce();
		expect(createWorker).toHaveBeenCalledOnce();
		expect(contextCreated).toHaveBeenCalledOnce();
		worker.reply("result", ready);
		await Promise.all([first, concurrent]);
		expect(service.prepare()).toBe(first);
		await service.prepare();
		expect(loadModel).toHaveBeenCalledOnce();
		expect(createWorker).toHaveBeenCalledOnce();
		expect(contextCreated).toHaveBeenCalledOnce();
		service.dispose();
		await expect(service.prepare()).rejects.toMatchObject({
			name: "AbortError",
		});
		expect(contextCreated).toHaveBeenCalledOnce();
	});
	it("preserves the same failed preparation promise/reason instead of retrying a disposed service", async () => {
		const { service, loadModel, createWorker } = setup({ language: "de" });
		const first = service.prepare();
		expect(service.prepare()).toBe(first);
		await expect(first).rejects.toThrow("English only");
		expect(service.prepare()).toBe(first);
		await expect(service.prepare()).rejects.toThrow("English only");
		expect(loadModel).not.toHaveBeenCalled();
		expect(createWorker).not.toHaveBeenCalled();
		expect(contextCreated).not.toHaveBeenCalled();
	});
	it("rejects preparation after disposal before first use without allocating resources", async () => {
		const { service, loadModel, createWorker } = setup();
		service.dispose();
		await expect(service.prepare()).rejects.toMatchObject({
			name: "AbortError",
		});
		expect(loadModel).not.toHaveBeenCalled();
		expect(createWorker).not.toHaveBeenCalled();
		expect(contextCreated).not.toHaveBeenCalled();
	});
	it("initializes a same-origin single-thread CPU worker with verified assets and no WebSocket", async () => {
		const { service, worker, createWorker, model } = await prepared();
		expect(createWorker.mock.calls[0]?.[0]?.origin).toBe(location.origin);
		expect(worker.postMessage).toHaveBeenCalledWith(
			expect.objectContaining({
				type: "init",
				options: expect.objectContaining({
					lang: "en",
					threads: 1,
					device: "cpu",
					quant: "q8",
					cache: false,
					preload: ["default"],
				}),
			}),
			expect.any(Array),
		);
		const init = worker.postMessage.mock.calls[0];
		expect(init?.[1]).toEqual([
			init?.[0].options.wasmUrl,
			init?.[0].options.simdWasmUrl,
		]);
		expect(init?.[0].options.wasmUrl).toBeInstanceOf(ArrayBuffer);
		expect(model.release).toHaveBeenCalledOnce();
		expect(WebSocket).not.toHaveBeenCalled();
		service.dispose();
		expect(worker.terminate).toHaveBeenCalledOnce();
		expect(close).toHaveBeenCalledOnce();
	});
	it.each([{ language: "de" }, { voice: "af_heart" }, { speed: Number.NaN }])(
		"fails closed before loading for %j",
		async (patch) => {
			const { service, loadModel, createWorker } = setup(patch);
			await expect(service.prepare()).rejects.toThrow();
			expect(loadModel).not.toHaveBeenCalled();
			expect(createWorker).not.toHaveBeenCalled();
			expect(WebSocket).not.toHaveBeenCalled();
		},
	);
	it("terminates preparation rather than just fencing late readiness", async () => {
		const { service, worker } = setup();
		const preparing = service.prepare();
		await tick();
		const late = worker.onmessage;
		service.dispose();
		await expect(preparing).rejects.toMatchObject({ name: "AbortError" });
		expect(worker.postMessage).toHaveBeenLastCalledWith({
			type: "cancel",
			id: 1,
		});
		expect(worker.terminate).toHaveBeenCalledOnce();
		late?.({ data: { type: "result", id: 1, value: ready } });
		await expect(service.speak("late")).rejects.toMatchObject({
			name: "AbortError",
		});
		expect(sources).toHaveLength(0);
	});
	it("aborts download preparation and releases assets arriving from a late loader", async () => {
		let deliver!: (model: PhononModelAssets) => void;
		let signal!: AbortSignal;
		const model = assets();
		const createWorker = vi.fn();
		const service = new PhononReadAloudService(
			{ language: "en", voice: "default", speed: 1 },
			{},
			{
				createWorker,
				loadModel: (abort) => {
					signal = abort;
					return new Promise((resolve) => {
						deliver = resolve;
					});
				},
			},
		);
		const preparing = service.prepare();
		await tick();
		service.dispose();
		expect(signal.aborted).toBe(true);
		deliver(model);
		await expect(preparing).rejects.toMatchObject({ name: "AbortError" });
		expect(model.release).toHaveBeenCalledOnce();
		expect(createWorker).not.toHaveBeenCalled();
	});
	it("streams immediately but resolves speak only after the last scheduled source drains", async () => {
		const { service, worker, onPlaying } = await prepared({ speed: 1.5 });
		let done = false;
		const speaking = service.speak("Synthetic test.").then(() => {
			done = true;
		});
		worker.chunk();
		expect(onPlaying).toHaveBeenCalledOnce();
		expect(sources[0]?.playbackRate.value).toBe(1.5);
		expect(sources[0]?.start).toHaveBeenCalledWith(0.01);
		worker.reply("result", { cancelled: false });
		await tick();
		expect(done).toBe(false);
		sources[0]?.end();
		await speaking;
		expect(done).toBe(true);
		service.dispose();
	});
	it("completes a zero-audio generation without inventing playback or waiting forever", async () => {
		const { service, worker, onPlaying } = await prepared();
		const speaking = service.speak("Synthetic.");
		worker.reply("result", { cancelled: false, samples: 0 });
		await speaking;
		expect(onPlaying).not.toHaveBeenCalled();
		expect(sources).toHaveLength(0);
		service.dispose();
	});
	it("worker crashes settle a pending generation and a fresh instance can retry", async () => {
		const { service, worker } = await prepared();
		const speaking = service.speak("Synthetic.");
		worker.onerror?.({
			preventDefault: vi.fn(),
			message: "synthetic crash",
		} as unknown as ErrorEvent);
		await expect(speaking).rejects.toThrow("synthetic crash");
		expect(worker.terminate).toHaveBeenCalledOnce();
		const retry = await prepared();
		retry.service.dispose();
	});
	it("does not resolve a generation whose buffers drained before inference ended", async () => {
		const { service, worker } = await prepared();
		let done = false;
		const speaking = service.speak("Synthetic.").then(() => {
			done = true;
		});
		worker.chunk();
		sources[0]?.end();
		await tick();
		expect(done).toBe(false);
		worker.reply("result", { cancelled: false });
		await speaking;
		service.dispose();
	});
	it.each([false, true])(
		"cancels inference/playback (generation ended: %s) and drops late chunks",
		async (ended) => {
			const { service, worker } = await prepared();
			const speaking = service.speak("Synthetic.");
			worker.chunk();
			const late = worker.onmessage;
			const id = worker.postMessage.mock.calls.at(-1)?.[0].id;
			if (ended) {
				worker.reply("result", { cancelled: false });
				await tick();
			}
			service.dispose();
			service.dispose();
			await expect(speaking).rejects.toMatchObject({ name: "AbortError" });
			late?.({ data: { type: "chunk", id, pcm: new Float32Array([0.1]) } });
			expect(sources).toHaveLength(1);
			expect(sources[0]?.stop).toHaveBeenCalledOnce();
			expect(worker.terminate).toHaveBeenCalledOnce();
			const retry = await prepared();
			retry.service.dispose();
		},
	);
	it("rejects worker failure, mismatched runtime language, and unsolicited cache writes", async () => {
		const initial = setup();
		const preparing = initial.service.prepare();
		await tick();
		initial.worker.reply("result", { ...ready, languages: ["de"] });
		await expect(preparing).rejects.toThrow("pinned English");
		const { service, worker } = await prepared();
		const speaking = service.speak("Synthetic.");
		worker.reply("store", {});
		await expect(speaking).rejects.toThrow("Unexpected");
		expect(worker.terminate).toHaveBeenCalledOnce();
	});
	it("fences errors and chunks with an unrelated request id", async () => {
		const { service, worker } = await prepared();
		const speaking = service.speak("Synthetic.");
		worker.reply("error", {}, 1000);
		worker.reply("result", { cancelled: false });
		await speaking;
		service.dispose();
	});
});
