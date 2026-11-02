import { RecognitionControls } from "@/components/voice/RecognitionControls";
import { DEFAULT_RECOGNITION_SETTINGS } from "@/lib/voice/recognition-settings";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

const base = {
	recognition: DEFAULT_RECOGNITION_SETTINGS,
	remoteAvailable: false,
	onProviderChange: vi.fn(),
	onLanguageChange: vi.fn(),
};

describe("wake phrase setting", () => {
	it("is hidden unless the server offers a verified release", () => {
		render(<RecognitionControls {...base} />);
		expect(screen.queryByLabelText(/wake phrase/i)).toBeNull();
		expect(screen.queryByText(/Start dictation on a wake phrase/)).toBeNull();
	});

	it("is off by default, explains what a detection can do, and toggles explicitly", () => {
		const onWakeChange = vi.fn();
		render(
			<RecognitionControls
				{...base}
				wakeAvailable
				wakeEnabled={false}
				onWakeChange={onWakeChange}
			/>,
		);
		const toggle = screen.getByLabelText(
			/Start dictation on a wake phrase/,
		) as HTMLInputElement;
		expect(toggle.checked).toBe(false);
		expect(screen.getByText(/Off by default/)).toBeTruthy();
		fireEvent.click(toggle);
		expect(onWakeChange).toHaveBeenCalledWith(true);
	});

	it("says a heard phrase only starts dictation and shows the muted state", () => {
		render(
			<RecognitionControls
				{...base}
				wakeAvailable
				wakeEnabled
				wakeState="muted"
			/>,
		);
		const note = screen.getByText(/only starts dictation/);
		expect(note.textContent).toContain("muted while speaking");
	});

	it("offers the phrase picker only when the release has more than one", () => {
		const { rerender } = render(
			<RecognitionControls
				{...base}
				wakeAvailable
				wakeEnabled
				wakePhrases={["hey_oqto"]}
				wakePhrase="hey_oqto"
			/>,
		);
		expect(screen.queryByLabelText("Wake phrase")).toBeNull();
		rerender(
			<RecognitionControls
				{...base}
				wakeAvailable
				wakeEnabled
				wakePhrases={["hey_oqto", "oqto"]}
				wakePhrase="oqto"
				onWakePhraseChange={vi.fn()}
			/>,
		);
		expect(
			(screen.getByLabelText("Wake phrase") as HTMLSelectElement).value,
		).toBe("oqto");
	});
});
