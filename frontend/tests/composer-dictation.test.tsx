import {
	DictationMicButton,
	DictationStatus,
	downloadDisplay,
} from "@/components/voice/ComposerDictation";
import type { UseDictationReturn } from "@/features/voice/hooks/useDictation";
import { initI18n } from "@/lib/i18n";
import { fireEvent, render, screen } from "@testing-library/react";
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
		recognition: { provider: "moonshine", language: "en" },
		remoteAvailable: false,
		setRecognitionProvider: vi.fn(),
		setRecognitionLanguage: vi.fn(),
		autoSendEnabled: false,
		setAutoSendEnabled: vi.fn(),
		start: vi.fn(async () => {}),
		stop: vi.fn(),
		cancel: vi.fn(),
		...patch,
	};
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
	it("keeps provider/language/auto-send settings in the microphone popover", () => {
		const dictation = sample({ isActive: false });
		render(<DictationMicButton dictation={dictation} />);
		expect(screen.queryByLabelText("Speech language")).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "Dictation settings" }));
		expect(screen.getByLabelText("Speech language")).toBeInTheDocument();
		expect(
			screen.getByRole("checkbox", { name: "Auto-send completed speech" }),
		).not.toBeChecked();
		fireEvent.click(screen.getByRole("button", { name: "Start dictation" }));
		expect(dictation.start).toHaveBeenCalledOnce();
	});
});
