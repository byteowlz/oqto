import { DictationOverlay } from "@/components/voice/DictationOverlay";
import { RecognitionControls } from "@/components/voice/RecognitionControls";
import { appendCompletedDraft } from "@/features/chat/hooks/draft-storage";
import { useDictation } from "@/features/voice/hooks/useDictation";
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
vi.mock("@/lib/voice", () => ({
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
					autoSendOnFinal: true,
					autoSendDelayMs: 50,
					onAutoSend: send,
				}),
			{ initialProps: { scopeKey: "workspace-a:session-a" } },
		);
		await act(() => result.current.start());
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
				autoSendOnFinal: true,
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
				autoSendOnFinal: true,
				onAutoSend: send,
			}),
		);
		await act(() => result.current.start());
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
				autoSendOnFinal: true,
				onAutoSend: send,
			}),
		);
		await act(() => result.current.start());
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
	it("turning auto-send off clears already scheduled work", async () => {
		vi.useFakeTimers();
		const send = vi.fn();
		const { result } = renderHook(() =>
			useDictation({
				config: null,
				onTranscript: vi.fn(),
				autoSendOnFinal: true,
				autoSendDelayMs: 1000,
				onAutoSend: send,
			}),
		);
		await act(() => result.current.start());
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
				recognition={{ provider: "moonshine", language: "en" }}
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
