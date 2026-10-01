import { STTService } from "@/lib/voice/stt-service";
import type { RecognitionEvent } from "@byteowlz/ears-browser";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const fake = vi.hoisted(() => ({
	listener: undefined as ((event: RecognitionEvent) => void) | undefined,
	prepare: vi.fn(),
	feed: vi.fn(),
	finish: vi.fn(),
	cancel: vi.fn(),
}));
vi.mock("@byteowlz/ears-browser", () => ({
	Recognizer: class {
		subscribe(listener: (event: RecognitionEvent) => void) {
			fake.listener = listener;
		}
		prepare = fake.prepare;
		feed = fake.feed;
		finish = fake.finish;
		cancel = fake.cancel;
	},
}));

beforeEach(() => {
	vi.clearAllMocks();
	Object.defineProperty(window, "isSecureContext", {
		configurable: true,
		value: true,
	});
	fake.prepare.mockImplementation(async () => {
		fake.listener?.({ type: "state", state: "ready" });
	});
});
afterEach(() => {
	vi.unstubAllGlobals();
});

it("uses local SDK options without a server, with opaque preview identities", async () => {
	const service = new STTService("", 1500, {
		provider: "moonshine",
		language: "de",
	});
	const preview = vi.fn();
	const final = vi.fn();
	service.setCallbacks({ onPreview: preview, onFinal: final });
	await service.connect();
	expect(fake.prepare).toHaveBeenCalledWith(
		expect.objectContaining({
			provider: "moonshine",
			language: "de",
			sampleRate: 24000,
		}),
	);
	fake.listener?.({
		type: "preview",
		id: "9999999999999999999",
		text: "wrong",
	});
	fake.listener?.({
		type: "preview",
		id: "9999999999999999999",
		text: "revised",
	});
	expect(preview).toHaveBeenLastCalledWith("revised");
	expect(final).not.toHaveBeenCalled();
	fake.listener?.({
		type: "completed",
		id: "9999999999999999999",
		text: "complete",
	});
	expect(service.getCurrentTranscript()).toBe("");
	expect(final).toHaveBeenCalledExactlyOnceWith("complete");
	service.disconnect();
});

it("forwards cumulative model bytes without summing files and clears progress when ready", async () => {
	const service = new STTService("", 1500, {
		provider: "moonshine",
		language: "en",
	});
	const download = vi.fn();
	service.setCallbacks({ onDownload: download });
	await service.connect();
	fake.listener?.({
		type: "progress",
		loaded: 4e6,
		total: 10e6,
		file: "encoder.ort",
	});
	fake.listener?.({
		type: "progress",
		loaded: 6e6,
		total: 10e6,
		file: "decoder_kv.ort",
	});
	expect(download).toHaveBeenLastCalledWith({ loaded: 6e6, total: 10e6 });
	fake.listener?.({ type: "state", state: "ready" });
	expect(download).toHaveBeenLastCalledWith(null);
	service.disconnect();
});

it("cancel during microphone permission releases the late-granted stream", async () => {
	let grant!: (stream: MediaStream) => void;
	const stop = vi.fn();
	Object.defineProperty(navigator, "mediaDevices", {
		configurable: true,
		value: {
			getUserMedia: vi.fn(
				() =>
					new Promise<MediaStream>((resolve) => {
						grant = resolve;
					}),
			),
		},
	});
	const service = new STTService("", 1500, {
		provider: "moonshine",
		language: "en",
	});
	const start = service.startListening();
	await Promise.resolve();
	await Promise.resolve();
	service.disconnect();
	grant({ getTracks: () => [{ stop }] } as unknown as MediaStream);
	await start;
	expect(stop).toHaveBeenCalledOnce();
	expect(service.getIsListening()).toBe(false);
});

it("AudioContext construction failure releases the already granted microphone", async () => {
	const stop = vi.fn();
	Object.defineProperty(navigator, "mediaDevices", {
		configurable: true,
		value: {
			getUserMedia: vi.fn(async () => ({ getTracks: () => [{ stop }] })),
		},
	});
	vi.stubGlobal(
		"AudioContext",
		class {
			constructor() {
				throw new Error("Capture context unsupported");
			}
		},
	);
	const service = new STTService("", 1500, {
		provider: "moonshine",
		language: "en",
	});
	await expect(service.startListening()).rejects.toThrow("unsupported");
	expect(stop).toHaveBeenCalledOnce();
	expect(service.getIsListening()).toBe(false);
});

it("preparation failure does not request microphone access", async () => {
	const getUserMedia = vi.fn();
	Object.defineProperty(navigator, "mediaDevices", {
		configurable: true,
		value: { getUserMedia },
	});
	fake.prepare.mockRejectedValueOnce(new Error("isolation required"));
	const service = new STTService("", 1500, {
		provider: "moonshine",
		language: "en",
	});
	await expect(service.startListening()).rejects.toThrow("isolation required");
	expect(getUserMedia).not.toHaveBeenCalled();
});
