import {
	DictationMicButton,
	DictationSettingsPanel,
	DictationStatus,
	downloadDisplay,
} from "@/components/voice/ComposerDictation";
import type { UseDictationReturn } from "@/features/voice/hooks/useDictation";
import { initI18n } from "@/lib/i18n";
import { DEFAULT_RECOGNITION_SETTINGS } from "@/lib/voice/recognition-settings";
import { WakeMeter } from "@/lib/voice/wake-meter";
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

initI18n();
function sample(patch: Partial<UseDictationReturn> = {}): UseDictationReturn {
	return {
		isActive: true,
		liveTranscript: "",
		vadProgress: 0,
		getInputVolume: () => 0.1,
		isConnected: true,
		error: null,
		dismissError: vi.fn(),
		preparation: null,
		download: null,
		recognition: { ...DEFAULT_RECOGNITION_SETTINGS },
		remoteAvailable: false,
		setRecognitionProvider: vi.fn(),
		microphones: [],
		refreshMicrophones: vi.fn(async () => {}),
		setWakeSilenceTimeoutMs: vi.fn(),
		setWakeSound: vi.fn(),
		setSendSound: vi.fn(),
		sendCountdown: null,
		voiceCue: null,
		setMicrophone: vi.fn(),
		setRecognitionLanguage: vi.fn(),
		autoSendEnabled: false,
		autoSendPauseMs: 2000,
		setAutoSendEnabled: vi.fn(),
		setAutoSendPauseMs: vi.fn(),
		start: vi.fn(async () => {}),
		stop: vi.fn(),
		cancel: vi.fn(),
		wakeAvailable: false,
		wakeState: "off",
		wakePhrases: [],
		setWakeEnabled: vi.fn(),
		setWakePhrase: vi.fn(),
		setWakeDebug: vi.fn(),
		setWakeThreshold: vi.fn(),
		wakeReleaseThreshold: null,
		wakeMeter: new WakeMeter(),
		wakeTriggerScore: null,
		setWakePlayback: vi.fn(),
		...patch,
	};
}

/** Both UIs render the microphone button and the settings overlay in the composer. */
function ComposerHarness({ dictation }: { dictation: UseDictationReturn }) {
	const [open, setOpen] = useState(false);
	return (
		<div>
			<DictationMicButton
				dictation={dictation}
				settingsOpen={open}
				onToggleSettings={() => setOpen((value) => !value)}
			/>
			<DictationSettingsPanel
				dictation={dictation}
				open={open}
				onClose={() => setOpen(false)}
			/>
		</div>
	);
}

describe("compact composer speech status", () => {
	it("shows whole-model bytes and percentage without duplicating cumulative bytes", () => {
		const { rerender } = render(
			<DictationStatus
				dictation={sample({
					preparation: "Downloading speech model…",
					download: { loaded: 4e6, total: 10e6 },
				})}
			/>,
		);
		expect(screen.getByRole("progressbar")).toHaveAttribute("value", "40");
		expect(screen.getByText("4 / 10 MB")).toBeInTheDocument();
		rerender(
			<DictationStatus
				dictation={sample({
					preparation: "Downloading speech model…",
					download: { loaded: 6e6, total: 10e6 },
				})}
			/>,
		);
		expect(screen.getByRole("progressbar")).toHaveAttribute("value", "60");
		expect(screen.getByText("6 / 10 MB")).toBeInTheDocument();
	});
	it("does not invent a percentage for unknown or inconsistent totals", () => {
		const { rerender } = render(
			<DictationStatus
				dictation={sample({
					preparation: "Downloading speech model…",
					download: { loaded: 4e6 },
				})}
			/>,
		);
		expect(screen.getByRole("progressbar")).not.toHaveAttribute("value");
		expect(screen.getByText("4 MB")).toBeInTheDocument();
		rerender(
			<DictationStatus
				dictation={sample({
					preparation: "Downloading speech model…",
					download: { loaded: 4e6, total: 2e6 },
				})}
			/>,
		);
		expect(screen.getByRole("progressbar")).not.toHaveAttribute("value");
		expect(downloadDisplay(Number.NaN, Number.POSITIVE_INFINITY)).toEqual({
			loaded: 0,
			total: undefined,
			percent: undefined,
		});
	});
	it("distinguishes initialization from listening and leaves Cancel usable", () => {
		const dictation = sample({
			preparation: "Downloading speech model…",
			download: { loaded: 10e6, total: 10e6 },
		});
		const { rerender } = render(<DictationStatus dictation={dictation} />);
		expect(screen.getByRole("status")).toHaveTextContent("Initializing model");
		expect(screen.getByRole("button", { name: "Finish" })).toBeDisabled();
		fireEvent.click(screen.getByRole("button", { name: "Cancel dictation" }));
		expect(dictation.cancel).toHaveBeenCalledOnce();
		rerender(<DictationStatus dictation={sample()} />);
		expect(screen.getByRole("status")).toHaveTextContent("Listening");
		expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
	});
	it("retains the exact editor node and its selection through download/listening/error/idle", () => {
		function Composer({ dictation }: { dictation: UseDictationReturn }) {
			return (
				<div className="composer-dictation-anchor">
					<textarea aria-label="Draft" defaultValue="typed draft" />
					<DictationStatus dictation={dictation} />
				</div>
			);
		}
		const { rerender } = render(
			<Composer dictation={sample({ isActive: false })} />,
		);
		const editor = screen.getByRole("textbox") as HTMLTextAreaElement;
		editor.setSelectionRange(2, 4);
		for (const state of [
			sample({ preparation: "Preparing…" }),
			sample({ liveTranscript: "tentative" }),
			sample({ error: "Failed", isActive: false }),
			sample({ isActive: false }),
		]) {
			rerender(<Composer dictation={state} />);
			expect(screen.getByRole("textbox")).toBe(editor);
			expect(editor).toHaveValue("typed draft");
			expect(editor.selectionStart).toBe(2);
			expect(editor.selectionEnd).toBe(4);
		}
	});
	it("samples volume in the leaf without rerendering the composer and releases its timer", () => {
		vi.useFakeTimers();
		try {
			let renders = 0;
			const readLevel = vi.fn(() => 0.2);
			function Host() {
				renders++;
				return (
					<DictationStatus dictation={sample({ getInputVolume: readLevel })} />
				);
			}
			const { unmount } = render(<Host />);
			const initial = renders;
			vi.advanceTimersByTime(660);
			expect(readLevel).toHaveBeenCalledTimes(11);
			expect(renders).toBe(initial);
			unmount();
			expect(vi.getTimerCount()).toBe(0);
		} finally {
			vi.useRealTimers();
		}
	});
	it("offers Retry and dismisses error on Cancel without changing the draft", () => {
		const dictation = sample({
			isActive: false,
			error: "Model download failed",
		});
		render(<DictationStatus dictation={dictation} />);
		expect(screen.getByRole("alert")).toHaveTextContent(
			"Model download failed",
		);
		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		expect(dictation.start).toHaveBeenCalledOnce();
		fireEvent.click(screen.getByRole("button", { name: "Cancel dictation" }));
		expect(dictation.dismissError).toHaveBeenCalledOnce();
	});
	it("keeps provider, language and auto-send in an overlay above the prompt box", () => {
		const dictation = sample({ isActive: false });
		render(<ComposerHarness dictation={dictation} />);
		const mic = screen.getByRole("button", { name: "Dictation settings" });
		expect(mic).toHaveAttribute("aria-expanded", "false");
		expect(screen.queryByLabelText("Speech language")).not.toBeInTheDocument();
		fireEvent.click(mic);
		// Not modal: a region in the composer layer, not a dialog.
		expect(screen.queryByRole("dialog")).toBeNull();
		expect(
			screen.getByRole("region", { name: "Dictation settings" }),
		).toBeInTheDocument();
		expect(mic).toHaveAttribute("aria-expanded", "true");
		expect(screen.getByLabelText("Speech language")).toBeInTheDocument();
		expect(
			screen.getByRole("checkbox", { name: "Send by itself after" }),
		).not.toBeChecked();
		fireEvent.click(screen.getByRole("button", { name: "Start dictation" }));
		expect(dictation.start).toHaveBeenCalledOnce();
	});

	it("offers the pause length only when auto-send is on, and explains it", () => {
		const dictation = sample({
			isActive: false,
			autoSendEnabled: true,
			autoSendPauseMs: 3000,
		});
		render(<ComposerHarness dictation={dictation} />);
		fireEvent.click(screen.getByRole("button", { name: "Dictation settings" }));
		const pause = screen.getByLabelText("Pause before sending");
		expect(pause).toHaveValue("3000");
		fireEvent.change(pause, { target: { value: "5000" } });
		expect(dictation.setAutoSendPauseMs).toHaveBeenCalledWith(5000);
		expect(screen.getByText(/Sends after 3 s without speech/)).toBeTruthy();
	});

	it("closes with Escape and with the close button without starting anything", () => {
		const dictation = sample({ isActive: false });
		render(<ComposerHarness dictation={dictation} />);
		fireEvent.click(screen.getByRole("button", { name: "Dictation settings" }));
		fireEvent.keyDown(
			screen.getByRole("region", { name: "Dictation settings" }),
			{ key: "Escape" },
		);
		expect(
			screen.queryByRole("region", { name: "Dictation settings" }),
		).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Dictation settings" }));
		fireEvent.click(screen.getByRole("button", { name: "Finish" }));
		expect(
			screen.queryByRole("region", { name: "Dictation settings" }),
		).toBeNull();
		expect(dictation.start).not.toHaveBeenCalled();
	});

	// Regression: the wake toggle existed only in RecognitionControls unit tests; the
	// composer popover both UIs render never passed the wake props, so it was unreachable.
	it("offers the wake toggle in the composer popover when the server has a release", () => {
		const dictation = sample({ isActive: false, wakeAvailable: true });
		render(<ComposerHarness dictation={dictation} />);
		fireEvent.click(screen.getByRole("button", { name: "Dictation settings" }));
		const toggle = screen.getByRole("checkbox", {
			name: "Start dictation on a wake phrase",
		});
		expect(toggle).not.toBeChecked();
		fireEvent.click(toggle);
		expect(dictation.setWakeEnabled).toHaveBeenCalledWith(true);
	});

	it("shows the manifest phrases once wake is on and the release has loaded", () => {
		const dictation = sample({
			isActive: false,
			wakeAvailable: true,
			wakeState: "listening",
			wakePhrases: ["hey_oqto", "oqto"],
			recognition: { ...DEFAULT_RECOGNITION_SETTINGS, wakeEnabled: true },
		});
		render(<ComposerHarness dictation={dictation} />);
		fireEvent.click(screen.getByRole("button", { name: "Dictation settings" }));
		const picker = screen.getByRole("combobox", { name: "Wake phrase" });
		fireEvent.change(picker, { target: { value: "oqto" } });
		expect(dictation.setWakePhrase).toHaveBeenCalledWith("oqto");
		expect(screen.getByText(/Listening locally/)).toBeInTheDocument();
	});

	it("hides wake entirely when the server offers no release", () => {
		render(<ComposerHarness dictation={sample({ isActive: false })} />);
		fireEvent.click(screen.getByRole("button", { name: "Dictation settings" }));
		expect(
			screen.queryByRole("checkbox", {
				name: "Start dictation on a wake phrase",
			}),
		).not.toBeInTheDocument();
	});

	it("marks the mic icon while the wake phrase is being listened for", () => {
		const listening = sample({
			isActive: false,
			wakeAvailable: true,
			wakeState: "listening",
			recognition: { ...DEFAULT_RECOGNITION_SETTINGS, wakeEnabled: true },
		});
		const { rerender } = render(<DictationMicButton dictation={listening} />);
		const button = screen.getByRole("button", { name: "Dictation settings" });
		expect(button).toHaveAttribute("data-wake", "listening");
		expect(button).toHaveAccessibleDescription('Listening for "hey oqto"');
		rerender(
			<DictationMicButton dictation={{ ...listening, wakeState: "muted" }} />,
		);
		expect(button).toHaveAttribute("data-wake", "muted");
		expect(button).toHaveAccessibleDescription(
			"Wake phrase paused while Oqto speaks",
		);
		rerender(
			<DictationMicButton
				dictation={{ ...listening, isActive: true, wakeState: "off" }}
			/>,
		);
		expect(button).not.toHaveAttribute("data-wake");
		rerender(
			<DictationMicButton
				dictation={sample({ isActive: false, wakeState: "off" })}
			/>,
		);
		expect(button).not.toHaveAttribute("data-wake");
		expect(button).not.toHaveAccessibleDescription();
	});

	it("drains a countdown bar over the configured pause while a send is pending", () => {
		render(
			<DictationStatus
				dictation={sample({
					isActive: true,
					sendCountdown: { startedAt: 1, durationMs: 3000 },
				})}
			/>,
		);
		const timer = screen.getByRole("timer", {
			name: "Time until the message is sent",
		});
		const bar = timer.querySelector("i") as HTMLElement;
		expect(bar.style.animationDuration).toBe("3000ms");
	});

	it("offers a send sound only while auto-send is on", () => {
		const off = sample({ isActive: false });
		const { unmount } = render(<ComposerHarness dictation={off} />);
		fireEvent.click(screen.getByRole("button", { name: "Dictation settings" }));
		expect(screen.queryByLabelText("Sound when sending")).toBeNull();
		unmount();
		const on = sample({ isActive: false, autoSendEnabled: true });
		render(<ComposerHarness dictation={on} />);
		fireEvent.click(screen.getByRole("button", { name: "Dictation settings" }));
		const select = screen.getByLabelText("Sound when sending");
		fireEvent.change(select, { target: { value: "pop" } });
		expect(on.setSendSound).toHaveBeenCalledWith("pop");
	});
});
