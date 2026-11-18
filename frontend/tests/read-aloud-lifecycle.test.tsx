import { useTTS, useTTSWithParagraphs } from "@/features/voice/hooks/useTTS";
import type { ReadAloudCallbacks } from "@/lib/voice/read-aloud-service";
import {
	DEFAULT_TTS_SETTINGS,
	TTS_SETTINGS_KEY,
	loadTTSSettings,
	parseTTSSettings,
	saveReadAloudSettings,
	saveTTSSettings,
	selectTTSProvider,
	useTTSSettings,
} from "@/lib/voice/tts-settings";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

function deferred() {
	let resolve!: () => void;
	let reject!: (error: Error) => void;
	const promise = new Promise<void>((yes, no) => {
		resolve = yes;
		reject = no;
	});
	return { promise, resolve, reject };
}
const mocks = vi.hoisted(() => ({ create: vi.fn() }));
vi.mock("@/lib/voice/read-aloud-service", () => ({
	createReadAloudService: mocks.create,
}));
const services: ReturnType<typeof service>[] = [];
function service(callbacks: ReadAloudCallbacks) {
	return {
		callbacks,
		preparation: deferred(),
		playback: deferred(),
		prepare: vi.fn(function (this: {
			preparation: ReturnType<typeof deferred>;
		}) {
			return this.preparation.promise;
		}),
		speak: vi.fn(function (this: { playback: ReturnType<typeof deferred> }) {
			return this.playback.promise;
		}),
		dispose: vi.fn(),
	};
}
const tick = async () => {
	for (let i = 0; i < 8; i++) await Promise.resolve();
};
beforeEach(() => {
	services.length = 0;
	vi.clearAllMocks();
	const storage = new Map<string, string>();
	vi.stubGlobal("localStorage", {
		getItem: vi.fn((key: string) => storage.get(key) ?? null),
		setItem: vi.fn((key: string, value: string) => {
			storage.set(key, value);
		}),
		clear: vi.fn(() => storage.clear()),
	});
	saveReadAloudSettings({
		...DEFAULT_TTS_SETTINGS,
		phonon: { ...DEFAULT_TTS_SETTINGS.phonon },
	});
	mocks.create.mockImplementation((_settings, callbacks) => {
		const instance = service(callbacks);
		services.push(instance);
		return instance;
	});
});
afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

describe("read-aloud settings and provider lifecycle", () => {
	it("migrates voice/speed without changing remote default and preserves per-provider choices", async () => {
		const legacy = parseTTSSettings('{"voice":"am_michael","speed":1.7}');
		expect(legacy).toMatchObject({
			provider: "kokorox",
			voice: "am_michael",
			speed: 1.7,
			phonon: { voice: "default", speed: 1, language: "en" },
		});
		saveTTSSettings(legacy);
		const hook = renderHook(() => useTTS());
		act(() => hook.result.current.setProvider("phonon"));
		await act(() => hook.result.current.setSpeed(0.8));
		await act(() => hook.result.current.setVoice("default"));
		expect(loadTTSSettings()).toMatchObject({
			voice: "am_michael",
			speed: 1.7,
			phonon: { speed: 0.8 },
		});
		act(() => hook.result.current.setProvider("kokorox"));
		expect(hook.result.current.settings.speed).toBe(1.7);
	});
	it("publishes same-tab choices to every mounted control without eagerly loading a model", () => {
		const a = renderHook(() => useTTS());
		const b = renderHook(() => useTTSSettings());
		act(() => selectTTSProvider("phonon"));
		expect(a.result.current.settings.provider).toBe("phonon");
		expect(b.result.current.provider).toBe("phonon");
		expect(mocks.create).not.toHaveBeenCalled();
	});
	it("stops pending preparation and fences late readiness/downloads; a subsequent play can retry", async () => {
		const hook = renderHook(() => useTTS());
		let old!: Promise<void>;
		act(() => {
			old = hook.result.current.speak("Synthetic.");
		});
		const prior = services[0];
		act(() => hook.result.current.stop());
		expect(prior?.dispose).toHaveBeenCalledOnce();
		await act(async () => {
			prior?.callbacks.onDownload?.({ loaded: 10, total: 20 });
			prior?.preparation.resolve();
			await expect(old).rejects.toMatchObject({ name: "AbortError" });
		});
		expect(hook.result.current.state).toBe("idle");
		expect(hook.result.current.download).toBeNull();
		let next!: Promise<void>;
		act(() => {
			next = hook.result.current.speak("Retry.");
		});
		await act(async () => {
			services[1]?.preparation.resolve();
			await tick();
		});
		await act(async () => {
			services[1]?.playback.resolve();
			await next;
		});
		expect(hook.result.current.state).toBe("idle");
	});
	it.each(["same-tab", "cross-tab"])(
		"provider changes during inference fence old playback (%s)",
		async (kind) => {
			const hook = renderHook(() => useTTS());
			let pending!: Promise<void>;
			act(() => {
				pending = hook.result.current.speak("Synthetic.");
			});
			await act(async () => {
				services[0]?.preparation.resolve();
				await tick();
			});
			act(() => {
				if (kind === "same-tab") selectTTSProvider("phonon");
				else {
					const next = JSON.stringify({
						...loadTTSSettings(),
						provider: "phonon",
					});
					localStorage.setItem(TTS_SETTINGS_KEY, next);
					window.dispatchEvent(
						new StorageEvent("storage", {
							key: TTS_SETTINGS_KEY,
							newValue: next,
						}),
					);
				}
			});
			expect(services[0]?.dispose).toHaveBeenCalledOnce();
			await act(async () => {
				services[0]?.callbacks.onPlaying?.();
				services[0]?.playback.reject(
					new DOMException("Playback stopped", "AbortError"),
				);
				await expect(pending).rejects.toMatchObject({ name: "AbortError" });
			});
			expect(hook.result.current.state).toBe("idle");
			expect(hook.result.current.settings.provider).toBe("phonon");
		},
	);
	it("keeps the explicit local choice in memory if persistence is denied", () => {
		const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
		const hook = renderHook(() => useTTS());
		vi.spyOn(localStorage, "setItem").mockImplementation(() => {
			throw new Error("blocked");
		});
		act(() => selectTTSProvider("phonon"));
		expect(loadTTSSettings().provider).toBe("phonon");
		expect(hook.result.current.settings.provider).toBe("phonon");
		expect(warn).toHaveBeenCalledWith(
			"[TTS] Settings could not be persisted",
			expect.any(Error),
		);
	});
	it("a stale legacy voice/speed editor cannot restore an old inference provider", () => {
		const stale = loadTTSSettings();
		selectTTSProvider("phonon");
		saveTTSSettings({ ...stale, voice: "am_michael", speed: 1.7 });
		expect(loadTTSSettings()).toMatchObject({
			provider: "phonon",
			voice: "am_michael",
			speed: 1.7,
			phonon: { voice: "default", speed: 1 },
		});
	});
	it("does not turn unknown persisted providers into remote fallback", () => {
		expect(parseTTSSettings('{"provider":"future-provider"}').provider).toBe(
			"unsupported",
		);
	});
	it("disposes on unmount and ignores a late ready callback", async () => {
		const hook = renderHook(() => useTTS());
		let pending!: Promise<void>;
		act(() => {
			pending = hook.result.current.speak("Synthetic.");
		});
		hook.unmount();
		expect(services[0]?.dispose).toHaveBeenCalledOnce();
		services[0]?.preparation.resolve();
		await expect(pending).rejects.toMatchObject({ name: "AbortError" });
		expect(services[0]?.speak).not.toHaveBeenCalled();
	});
	it("does not advance paragraphs before playback drains, or after a provider-switch cancellation", async () => {
		const hook = renderHook(() => useTTSWithParagraphs("First.\n\nSecond."));
		act(() => hook.result.current.play());
		await act(async () => {
			services[0]?.preparation.resolve();
			await tick();
		});
		expect(services[0]?.speak).toHaveBeenCalledExactlyOnceWith("First.");
		await act(async () => {
			await tick();
		});
		expect(hook.result.current.currentParagraph).toBe(0);
		act(() => selectTTSProvider("phonon"));
		await act(async () => {
			services[0]?.playback.reject(
				new DOMException("Playback stopped", "AbortError"),
			);
			await tick();
		});
		expect(services[0]?.speak).toHaveBeenCalledTimes(1);
		expect(hook.result.current.isReading).toBe(false);
	});
	it("advances to the next paragraph only when the prior speak promise drains", async () => {
		const hook = renderHook(() => useTTSWithParagraphs("First.\n\nSecond."));
		act(() => hook.result.current.play());
		const first = deferred();
		const second = deferred();
		services[0]?.speak
			.mockImplementationOnce(() => first.promise)
			.mockImplementationOnce(() => second.promise);
		await act(async () => {
			services[0]?.preparation.resolve();
			await tick();
		});
		expect(services[0]?.speak).toHaveBeenCalledExactlyOnceWith("First.");
		await act(async () => {
			first.resolve();
			await tick();
		});
		expect(services[0]?.speak).toHaveBeenLastCalledWith("Second.");
		expect(hook.result.current.currentParagraph).toBe(1);
		expect(hook.result.current.isReading).toBe(true);
		expect(services[0]?.dispose).not.toHaveBeenCalled();
		await act(async () => {
			second.resolve();
			await tick();
		});
		expect(hook.result.current.isReading).toBe(false);
		expect(services[0]?.dispose).toHaveBeenCalledOnce();
	});
	it("propagates real failures so paragraph playback stops rather than silently advancing", async () => {
		const logged = vi.spyOn(console, "error").mockImplementation(() => {});
		const hook = renderHook(() => useTTSWithParagraphs("First.\n\nSecond."));
		act(() => hook.result.current.play());
		await act(async () => {
			services[0]?.preparation.reject(new Error("model unavailable"));
			await tick();
		});
		expect(hook.result.current.state).toBe("error");
		expect(hook.result.current.error).toBe("model unavailable");
		expect(logged).toHaveBeenCalledWith(
			"[TTS] speakParagraph error:",
			expect.any(Error),
		);
		expect(hook.result.current.currentParagraph).toBe(0);
		expect(hook.result.current.isReading).toBe(false);
	});
});
