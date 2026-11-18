import { DictationOverlay } from "@/components/voice/DictationOverlay";
import { RecognitionControls } from "@/components/voice/RecognitionControls";
import { useDictation } from "@/features/voice/hooks/useDictation";
import { appendCompletedDraft } from "@/lib/chat-draft-storage";
import { DEFAULT_RECOGNITION_SETTINGS } from "@/lib/voice/recognition-settings";
import type {
	STTCallbacks,
	STTRecognitionSettings,
} from "@/lib/voice/stt-service";
import {
	act,
	fireEvent,
	render,
	renderHook,
	screen,
} from "@testing-library/react";
import { useLayoutEffect } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const instances: FakeService[] = [];
class FakeService {
	callbacks: STTCallbacks = {};
	listening = false;
	constructor(
		public url: string,
		_timeout: number,
		public selection: STTRecognitionSettings,
	) {
		instances.push(this);
	}
	setCallbacks(callbacks: STTCallbacks) {
		this.callbacks = callbacks;
	}
	async startListening() {
		this.listening = true;
		this.callbacks.onConnectionChange?.(true);
	}
	async finishListening() {
		this.callbacks.onFinal?.("engine final");
		this.listening = false;
	}
	disconnect() {
		this.listening = false;
	}
	getInputVolume() {
		return 0;
	}
}
vi.mock("@/lib/voice/stt-service", () => ({
	STTService: vi
		.fn()
		.mockImplementation(
			(...args: ConstructorParameters<typeof FakeService>) =>
				new FakeService(...args),
		),
}));
vi.mock("@/lib/control-plane-client", () => ({
	voiceProxyWsUrl: vi.fn(() => "wss://example.test/voice/stt"),
}));

beforeEach(() => {
	instances.length = 0;
	const storage = new Map<string, string>();
	vi.mocked(localStorage.getItem).mockImplementation(
		(key) => storage.get(key) ?? null,
	);
	vi.mocked(localStorage.setItem).mockImplementation((key, value) => {
		storage.set(key, value);
	});
	vi.mocked(localStorage.clear).mockImplementation(() => storage.clear());
	vi.useRealTimers();
});

describe("standalone Oqto dictation", () => {
	it("stops capture and fences finals and scheduled sends when the composer scope changes", async () => {
		vi.useFakeTimers();
		const insert = vi.fn();
		const send = vi.fn();
		const { result, rerender } = renderHook(
			({ scopeKey }) =>
				useDictation({
					scopeKey,
					config: null,
					onTranscript: insert,
					onAutoSend: send,
				}),
			{ initialProps: { scopeKey: "workspace-a:session-a" } },
		);
		await act(() => result.current.start());
		act(() => {
			result.current.setAutoSendEnabled(true);
			result.current.setAutoSendPauseMs(50);
		});
		act(() => instances[0]?.callbacks.onFinal?.("completed in A"));
		rerender({ scopeKey: "workspace-a:session-b" });
		act(() => {
			instances[0]?.callbacks.onPreview?.("late preview");
			instances[0]?.callbacks.onFinal?.("late final");
			vi.advanceTimersByTime(50);
		});
		expect(result.current.isActive).toBe(false);
		expect(instances[0]?.listening).toBe(false);
		expect(result.current.liveTranscript).toBe("");
		expect(insert).toHaveBeenCalledExactlyOnceWith("completed in A");
		expect(send).not.toHaveBeenCalled();
	});
	it("fences old finals during layout before passive scope cleanup", async () => {
		const insert = vi.fn();
		const { result, rerender } = renderHook(
			({ scopeKey }) => {
				const dictation = useDictation({
					scopeKey,
					config: null,
					onTranscript: insert,
				});
				useLayoutEffect(() => {
					if (scopeKey === "B") instances[0]?.callbacks.onFinal?.("old result");
				}, [scopeKey]);
				return dictation;
			},
			{ initialProps: { scopeKey: "A" } },
		);
		await act(() => result.current.start());
		rerender({ scopeKey: "B" });
		expect(insert).not.toHaveBeenCalled();
		expect(instances[0]?.listening).toBe(false);
		await act(() => result.current.start());
		act(() => instances[1]?.callbacks.onFinal?.("new result"));
		expect(insert).toHaveBeenCalledExactlyOnceWith("new result");
	});
	it("a finishing old engine cannot deliver into or cancel a new composer capture", async () => {
		const insert = vi.fn();
		const { result, rerender } = renderHook(
			({ scopeKey }) =>
				useDictation({ scopeKey, config: null, onTranscript: insert }),
			{ initialProps: { scopeKey: "A" } },
		);
		await act(() => result.current.start());
		let finish!: () => void;
		const oldService = instances[0];
		if (!oldService) throw new Error("Capture service was not created");
		vi.spyOn(oldService, "finishListening").mockImplementation(
			() =>
				new Promise<void>((resolve) => {
					finish = resolve;
				}),
		);
		act(() => result.current.stop());
		rerender({ scopeKey: "B" });
		await act(() => result.current.start());
		await act(async () => {
			instances[0]?.callbacks.onFinal?.("stale drained final");
			finish();
			await Promise.resolve();
		});
		expect(insert).not.toHaveBeenCalled();
		expect(result.current.isActive).toBe(true);
		expect(instances[1]?.listening).toBe(true);
	});
	it("starts local recognition without voice config; previews never insert or auto-send", async () => {
		const insert = vi.fn();
		const send = vi.fn();
		const { result } = renderHook(() =>
			useDictation({
				config: null,
				onTranscript: insert,
				onAutoSend: send,
			}),
		);
		await act(() => result.current.start());
		expect(instances[0]?.url).toBe("");
		expect(instances[0]?.selection.provider).toBe("moonshine");
		act(() => instances[0]?.callbacks.onPreview?.("tentative"));
		expect(result.current.liveTranscript).toBe("tentative");
		expect(insert).not.toHaveBeenCalled();
		expect(send).not.toHaveBeenCalled();
		act(() => result.current.cancel());
		act(() => instances[0]?.callbacks.onFinal?.("late"));
		expect(insert).not.toHaveBeenCalled();
		expect(result.current.isActive).toBe(false);
	});
	it("retains completed speech on reload after Finish without persisting previews", async () => {
		const key = "workspace:session:a:draft";
		const otherKey = "workspace:session:b:draft";
		localStorage.setItem(key, "typed draft");
		localStorage.setItem(otherKey, "other draft");
		let draft = localStorage.getItem(key) ?? "";
		const send = vi.fn();
		const { result, unmount } = renderHook(() =>
			useDictation({
				scopeKey: key,
				config: null,
				onTranscript: (text) => {
					draft = appendCompletedDraft(key, draft, text);
				},
				onAutoSend: send,
			}),
		);
		await act(() => result.current.start());
		act(() => result.current.setAutoSendEnabled(true));
		act(() => instances[0]?.callbacks.onPreview?.("wrong preview"));
		expect(localStorage.getItem(key)).toBe("typed draft");
		await act(async () => {
			result.current.stop();
			await Promise.resolve();
		});
		unmount();
		expect(localStorage.getItem(key)).toBe("typed draft engine final");
		expect(localStorage.getItem(otherKey)).toBe("other draft");
		expect(send).not.toHaveBeenCalled();
	});
	it("inserts a drained engine final once, without auto-send on manual Finish", async () => {
		const insert = vi.fn();
		const send = vi.fn();
		const { result } = renderHook(() =>
			useDictation({
				config: null,
				onTranscript: insert,
				onAutoSend: send,
			}),
		);
		await act(() => result.current.start());
		act(() => result.current.setAutoSendEnabled(true));
		act(() => instances[0]?.callbacks.onPreview?.("wrong preview"));
		await act(async () => {
			result.current.stop();
			await Promise.resolve();
		});
		expect(insert).toHaveBeenCalledExactlyOnceWith("engine final");
		expect(send).not.toHaveBeenCalled();
		expect(result.current.liveTranscript).toBe("");
	});
	it("changing language fences the previous provider and preserves settings across remount", async () => {
		const insert = vi.fn();
		const { result, unmount } = renderHook(() =>
			useDictation({ config: null, onTranscript: insert }),
		);
		await act(() => result.current.start());
		act(() => result.current.setRecognitionLanguage("de"));
		act(() => instances[0]?.callbacks.onFinal?.("stale"));
		expect(insert).not.toHaveBeenCalled();
		unmount();
		const next = renderHook(() =>
			useDictation({ config: null, onTranscript: insert }),
		);
		expect(next.result.current.recognition.language).toBe("de");
	});
	it("cross-tab provider changes stop the old destination rather than relabeling live capture", async () => {
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: vi.fn() }),
		);
		await act(() => result.current.start());
		act(() =>
			window.dispatchEvent(
				new StorageEvent("storage", {
					key: "oqto-recognition-settings",
					newValue: JSON.stringify({ provider: "ears", language: "de" }),
				}),
			),
		);
		expect(result.current.recognition.provider).toBe("ears");
		expect(result.current.isActive).toBe(false);
		expect(instances[0]?.listening).toBe(false);
	});
	it("drops model progress on cancellation and fences late download callbacks", async () => {
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: vi.fn() }),
		);
		await act(() => result.current.start());
		act(() =>
			instances[0]?.callbacks.onDownload?.({ loaded: 6e6, total: 10e6 }),
		);
		expect(result.current.download).toEqual({ loaded: 6e6, total: 10e6 });
		act(() => result.current.cancel());
		act(() =>
			instances[0]?.callbacks.onDownload?.({ loaded: 8e6, total: 10e6 }),
		);
		expect(result.current.download).toBeNull();
	});
	it("sends once after a pause, not per completed sentence", async () => {
		vi.useFakeTimers();
		const insert = vi.fn();
		const send = vi.fn();
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: insert, onAutoSend: send }),
		);
		await act(() => result.current.start());
		act(() => {
			result.current.setAutoSendEnabled(true);
			result.current.setAutoSendPauseMs(2000);
		});
		act(() => instances[0]?.callbacks.onFinal?.("first sentence."));
		act(() => vi.advanceTimersByTime(1200));
		// New speech restarts the pause instead of sending the first sentence.
		act(() => instances[0]?.callbacks.onPreview?.("second"));
		act(() => instances[0]?.callbacks.onFinal?.("second sentence."));
		act(() => vi.advanceTimersByTime(1500));
		expect(send).not.toHaveBeenCalled();
		act(() => vi.advanceTimersByTime(500));
		expect(send).toHaveBeenCalledOnce();
		expect(insert).toHaveBeenCalledTimes(2);
	});

	it("ends the dictation after sending so the wake phrase listens again", async () => {
		vi.useFakeTimers();
		const send = vi.fn();
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: vi.fn(), onAutoSend: send }),
		);
		await act(() => result.current.start());
		act(() => {
			result.current.setAutoSendEnabled(true);
			result.current.setAutoSendPauseMs(1000);
		});
		expect(result.current.isActive).toBe(true);
		act(() => instances[0]?.callbacks.onFinal?.("send me."));
		act(() => vi.advanceTimersByTime(1000));
		expect(send).toHaveBeenCalledOnce();
		expect(result.current.isActive).toBe(false);
		expect(instances[0]?.listening).toBe(false);
		// A late engine final cannot re-send; the capture is already closed.
		act(() => instances[0]?.callbacks.onFinal?.("late"));
		act(() => vi.advanceTimersByTime(5000));
		expect(send).toHaveBeenCalledOnce();
	});

	it("keeps the configured pause across dictations", async () => {
		vi.useFakeTimers();
		const send = vi.fn();
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: vi.fn(), onAutoSend: send }),
		);
		await act(() => result.current.start());
		act(() => {
			result.current.setAutoSendEnabled(true);
			result.current.setAutoSendPauseMs(5000);
		});
		expect(result.current.autoSendPauseMs).toBe(5000);
		act(() => instances[0]?.callbacks.onFinal?.("one."));
		act(() => vi.advanceTimersByTime(4000));
		expect(send).not.toHaveBeenCalled();
		act(() => vi.advanceTimersByTime(1000));
		expect(send).toHaveBeenCalledOnce();
	});

	it("remembers auto-send and its pause as dictation settings", async () => {
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: vi.fn() }),
		);
		act(() => {
			result.current.setAutoSendEnabled(true);
			result.current.setAutoSendPauseMs(3000);
		});
		const stored = JSON.parse(
			localStorage.getItem("oqto-recognition-settings") ?? "{}",
		);
		expect(stored).toMatchObject({
			autoSendEnabled: true,
			autoSendPauseMs: 3000,
			microphoneId: null,
		});
	});

	it("with wake on, a dictation finishes by itself after the configured silence", async () => {
		vi.useFakeTimers();
		const insert = vi.fn();
		const send = vi.fn();
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: insert, onAutoSend: send }),
		);
		act(() => {
			result.current.setWakeEnabled(true);
			result.current.setWakeSilenceTimeoutMs(10_000);
		});
		await act(() => result.current.start());
		expect(result.current.isActive).toBe(true);
		// Speech extends the window.
		act(() => vi.advanceTimersByTime(8_000));
		act(() => instances[0]?.callbacks.onPreview?.("still talking"));
		act(() => vi.advanceTimersByTime(8_000));
		expect(result.current.isActive).toBe(true);
		// Silence ends it: finished (engine final delivered), not cancelled, not sent.
		await act(async () => {
			vi.advanceTimersByTime(2_000);
			await Promise.resolve();
		});
		expect(insert).toHaveBeenCalledWith("engine final");
		expect(result.current.isActive).toBe(false);
		expect(instances[0]?.listening).toBe(false);
		expect(send).not.toHaveBeenCalled();
	});

	it("without wake, or with 'never', dictation keeps listening", async () => {
		vi.useFakeTimers();
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: vi.fn() }),
		);
		await act(() => result.current.start());
		act(() => vi.advanceTimersByTime(120_000));
		expect(result.current.isActive).toBe(true);
		act(() => result.current.cancel());
		act(() => {
			result.current.setWakeEnabled(true);
			result.current.setWakeSilenceTimeoutMs(0);
		});
		await act(() => result.current.start());
		act(() => vi.advanceTimersByTime(120_000));
		expect(result.current.isActive).toBe(true);
	});

	it("exposes the pending send as a countdown that speech resets and sending clears", async () => {
		vi.useFakeTimers();
		const send = vi.fn();
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: vi.fn(), onAutoSend: send }),
		);
		await act(() => result.current.start());
		act(() => {
			result.current.setAutoSendEnabled(true);
			result.current.setAutoSendPauseMs(2000);
		});
		expect(result.current.sendCountdown).toBeNull();
		act(() => instances[0]?.callbacks.onFinal?.("first."));
		const first = result.current.sendCountdown;
		expect(first?.durationMs).toBe(2000);
		// New speech cancels the pending send; the next final starts a fresh one.
		act(() => vi.advanceTimersByTime(1000));
		act(() => instances[0]?.callbacks.onPreview?.("more"));
		expect(result.current.sendCountdown).toBeNull();
		act(() => instances[0]?.callbacks.onFinal?.("second."));
		expect(result.current.sendCountdown?.startedAt).toBeGreaterThan(
			first?.startedAt ?? 0,
		);
		act(() => vi.advanceTimersByTime(2000));
		expect(send).toHaveBeenCalledOnce();
		expect(result.current.sendCountdown).toBeNull();
	});

	it("reports a 'sent' cue when an auto-send fires, for the mascot", async () => {
		vi.useFakeTimers();
		const { result } = renderHook(() =>
			useDictation({
				config: null,
				onTranscript: vi.fn(),
				onAutoSend: vi.fn(),
			}),
		);
		await act(() => result.current.start());
		act(() => {
			result.current.setAutoSendEnabled(true);
			result.current.setAutoSendPauseMs(1000);
		});
		expect(result.current.voiceCue).toBeNull();
		act(() => instances[0]?.callbacks.onFinal?.("send it."));
		act(() => vi.advanceTimersByTime(1000));
		expect(result.current.voiceCue?.kind).toBe("sent");
	});

	it("turning auto-send off clears already scheduled work", async () => {
		vi.useFakeTimers();
		const send = vi.fn();
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: vi.fn(), onAutoSend: send }),
		);
		await act(() => result.current.start());
		act(() => {
			result.current.setAutoSendEnabled(true);
			result.current.setAutoSendPauseMs(1000);
		});
		act(() => instances[0]?.callbacks.onFinal?.("completed"));
		act(() => result.current.setAutoSendEnabled(false));
		act(() => vi.advanceTimersByTime(1000));
		expect(send).not.toHaveBeenCalled();
	});
	it("remote choice fails closed without configuration; local mode is not silently selected", async () => {
		localStorage.setItem(
			"oqto-recognition-settings",
			JSON.stringify({ provider: "ears", language: "de" }),
		);
		const { result } = renderHook(() =>
			useDictation({ config: null, onTranscript: vi.fn() }),
		);
		await act(() => result.current.start());
		expect(result.current.error).toContain("not configured");
		expect(result.current.isActive).toBe(false);
		expect(result.current.recognition.provider).toBe("ears");
	});
});

describe("dictation surface", () => {
	it("keeps provisional text outside the editable/sent composer and exposes preparation", () => {
		render(
			<DictationOverlay
				open
				value="committed"
				liveTranscript="tentative"
				preparation="Loading speech model: 40%"
				vadProgress={0}
				autoSend={false}
				onAutoSendChange={vi.fn()}
				onStop={vi.fn()}
				onChange={vi.fn()}
			/>,
		);
		expect(screen.getByRole("textbox")).toHaveValue("committed");
		expect(screen.getByLabelText("Provisional transcript")).toHaveTextContent(
			"tentative",
		);
		expect(screen.getByRole("status")).toHaveTextContent("40%");
		expect(
			screen.getByRole("button", { name: "Cancel dictation" }),
		).toBeEnabled();
	});
	it("local/remote and language controls are labeled and unavailable remote is disabled", () => {
		const language = vi.fn();
		render(
			<RecognitionControls
				recognition={{ ...DEFAULT_RECOGNITION_SETTINGS }}
				remoteAvailable={false}
				onProviderChange={vi.fn()}
				onLanguageChange={language}
			/>,
		);
		expect(
			screen.getByRole("option", { name: /not configured/ }),
		).toBeDisabled();
		fireEvent.change(screen.getByLabelText("Speech language"), {
			target: { value: "de" },
		});
		expect(language).toHaveBeenCalledWith("de");
	});
});
