import { DictationOverlay } from "@/components/voice/DictationOverlay";
import { RecognitionControls } from "@/components/voice/RecognitionControls";
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
