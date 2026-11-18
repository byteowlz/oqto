import { createReadAloudService } from "@/lib/voice/read-aloud-service";
import { DEFAULT_TTS_SETTINGS } from "@/lib/voice/tts-settings";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const fake = vi.hoisted(() => ({
	callbacks: {} as {
		onStopped?: () => void;
		onConnectionChange?: (connected: boolean) => void;
	},
	connect: vi.fn(),
	speak: vi.fn(),
	setVoice: vi.fn(),
	setSpeed: vi.fn(),
	disconnect: vi.fn(),
}));
vi.mock("@/lib/api/files", () => ({
	voiceProxyWsUrl: vi.fn(() => "wss://example.test/api/voice/tts"),
}));
vi.mock("@/lib/voice/tts-service", () => ({
	TTSService: class {
		setCallbacks(callbacks: typeof fake.callbacks) {
			fake.callbacks = callbacks;
		}
		connect = fake.connect;
		speak = fake.speak;
		setVoice = fake.setVoice;
		setSpeed = fake.setSpeed;
		disconnect = fake.disconnect;
	},
}));
beforeEach(() => {
	vi.clearAllMocks();
	fake.callbacks = {};
	fake.connect.mockResolvedValue(undefined);
	fake.setVoice.mockResolvedValue(undefined);
	fake.setSpeed.mockResolvedValue(undefined);
	fake.speak.mockResolvedValue(undefined);
});
afterEach(() => vi.restoreAllMocks());
it("retains Kokorox default/settings, but waits for playback drainage after synthesis", async () => {
	const service = createReadAloudService(DEFAULT_TTS_SETTINGS, {});
	await service.prepare();
	expect(fake.setVoice).toHaveBeenCalledWith("af_heart");
	expect(fake.setSpeed).toHaveBeenCalledWith(1.3);
	let done = false;
	const speaking = service.speak("Synthetic.").then(() => {
		done = true;
	});
	await Promise.resolve();
	await Promise.resolve();
	expect(done).toBe(false);
	fake.callbacks.onStopped?.();
	await speaking;
	service.dispose();
});
it("settles pending preparation immediately on dispose even when connect never returns", async () => {
	fake.connect.mockReturnValue(new Promise(() => {}));
	const service = createReadAloudService(DEFAULT_TTS_SETTINGS, {});
	const preparing = service.prepare();
	service.dispose();
	await expect(preparing).rejects.toMatchObject({ name: "AbortError" });
	expect(fake.disconnect).toHaveBeenCalledOnce();
});
it("rejects pending playback on disconnect and permits a fresh explicit retry", async () => {
	const service = createReadAloudService(DEFAULT_TTS_SETTINGS, {});
	await service.prepare();
	const speaking = service.speak("Synthetic.");
	fake.callbacks.onConnectionChange?.(false);
	await expect(speaking).rejects.toThrow("disconnected");
	expect(fake.disconnect).toHaveBeenCalledOnce();
	const retry = createReadAloudService(DEFAULT_TTS_SETTINGS, {});
	await retry.prepare();
	retry.dispose();
});
it("rejects concurrent speaks without orphaning the active drainage promise", async () => {
	const service = createReadAloudService(DEFAULT_TTS_SETTINGS, {});
	await service.prepare();
	const first = service.speak("First.");
	await expect(service.speak("Second.")).rejects.toThrow("already speaking");
	fake.callbacks.onStopped?.();
	await first;
	service.dispose();
});
it("disposes a failed preparation before explicit retry", async () => {
	fake.connect.mockRejectedValueOnce(new Error("unavailable"));
	const service = createReadAloudService(DEFAULT_TTS_SETTINGS, {});
	await expect(service.prepare()).rejects.toThrow("unavailable");
	expect(fake.disconnect).toHaveBeenCalledOnce();
	const retry = createReadAloudService(DEFAULT_TTS_SETTINGS, {});
	await retry.prepare();
	retry.dispose();
});
it("rejects unsupported persisted providers without connecting", () => {
	expect(() =>
		createReadAloudService(
			{ ...DEFAULT_TTS_SETTINGS, provider: "unsupported" },
			{},
		),
	).toThrow("Unsupported");
	expect(fake.connect).not.toHaveBeenCalled();
});
