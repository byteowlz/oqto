import { TTSService } from "@/lib/voice/tts-service";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

class Socket {
	static OPEN = 1;
	static CONNECTING = 0;
	readyState = 1;
	onopen?: () => void;
	onmessage?: (event: { data: string }) => void;
	onclose?: () => void;
	send = vi.fn();
	close = vi.fn();
	constructor() {
		sockets.push(this);
	}
	emit(data: unknown) {
		this.onmessage?.({ data: JSON.stringify(data) });
	}
}
const sockets: Socket[] = [];
const sources: {
	onended?: () => void;
	start: ReturnType<typeof vi.fn>;
	stop: ReturnType<typeof vi.fn>;
	disconnect: ReturnType<typeof vi.fn>;
}[] = [];
let decoded!: (buffer: AudioBuffer) => void;
class Context {
	state = "running";
	currentTime = 0;
	destination = {};
	resume = vi.fn(async () => {});
	close = vi.fn(async () => {});
	decodeAudioData = vi.fn(
		() =>
			new Promise<AudioBuffer>((resolve) => {
				decoded = resolve;
			}),
	);
	createAnalyser() {
		return { connect: vi.fn(), fftSize: 256, smoothingTimeConstant: 0 };
	}
	createBufferSource() {
		const source = {
			connect: vi.fn(),
			start: vi.fn(),
			stop: vi.fn(),
			disconnect: vi.fn(),
		};
		sources.push(source);
		return source;
	}
}
async function connected() {
	const service = new TTSService("wss://example.test/tts");
	const connecting = service.connect();
	sockets[0]?.onopen?.();
	await connecting;
	return service;
}
beforeEach(() => {
	sockets.length = 0;
	sources.length = 0;
	vi.useFakeTimers();
	vi.stubGlobal("WebSocket", Socket);
	vi.stubGlobal("AudioContext", Context);
	vi.spyOn(console, "log").mockImplementation(() => {});
});
afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
	vi.restoreAllMocks();
});
it("does not report playback drained while decode or scheduled sources are pending", async () => {
	const service = await connected();
	const stopped = vi.fn();
	service.setCallbacks({ onStopped: stopped });
	const speaking = service.speak("Synthetic.");
	sockets[0]?.emit({ type: "audio_chunk", chunk: "AA==" });
	sockets[0]?.emit({ type: "synthesis_completed" });
	await speaking;
	await vi.advanceTimersByTimeAsync(500);
	expect(stopped).not.toHaveBeenCalled();
	decoded({ duration: 1 } as AudioBuffer);
	await vi.advanceTimersByTimeAsync(500);
	expect(stopped).not.toHaveBeenCalled();
	sources[0]?.onended?.();
	await vi.advanceTimersByTimeAsync(200);
	expect(stopped).toHaveBeenCalledOnce();
	service.disconnect();
});
it("a zero-audio synthesis still reports drainage exactly once", async () => {
	const service = await connected();
	const stopped = vi.fn();
	service.setCallbacks({ onStopped: stopped });
	const speaking = service.speak("Synthetic.");
	sockets[0]?.emit({ type: "synthesis_completed" });
	await speaking;
	await vi.advanceTimersByTimeAsync(500);
	expect(stopped).toHaveBeenCalledOnce();
	service.disconnect();
});
it("a late audio decode from stopped playback cannot enter a new synthesis", async () => {
	const service = await connected();
	const first = service.speak("First.");
	sockets[0]?.emit({ type: "audio_chunk", chunk: "AA==" });
	service.stopPlayback();
	await expect(first).rejects.toThrow("Playback stopped");
	const second = service.speak("Second.");
	decoded({ duration: 1 } as AudioBuffer);
	await Promise.resolve();
	expect(sources).toHaveLength(0);
	sockets[0]?.emit({ type: "synthesis_completed" });
	await second;
	service.disconnect();
});
