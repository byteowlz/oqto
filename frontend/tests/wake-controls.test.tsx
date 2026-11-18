import { RecognitionControls } from "@/components/voice/RecognitionControls";
import { initI18n } from "@/lib/i18n";
import { DEFAULT_RECOGNITION_SETTINGS } from "@/lib/voice/recognition-settings";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

initI18n();

const base = {
	recognition: DEFAULT_RECOGNITION_SETTINGS,
	remoteAvailable: false,
	onProviderChange: vi.fn(),
	onLanguageChange: vi.fn(),
};

describe("wake phrase setting", () => {
	it("names the two recognition choices in plain language and explains the trade-off", () => {
		render(<RecognitionControls {...base} />);
		const select = screen.getAllByRole("combobox")[0] as HTMLSelectElement;
		const labels = Array.from(select.querySelectorAll("option")).map(
			(option) => option.textContent,
		);
		expect(labels[0]).toBe("On this device");
		expect(labels[1]).toContain("On your speech server");
		// No vendor names in the choice itself.
		expect(labels.join(" ")).not.toMatch(/moonshine|ears/i);
		expect(
			screen.getByRole("button", { name: "What the two options mean" }),
		).toBeTruthy();
	});

	it("lists input devices and keeps the system default selectable", () => {
		const onMicrophoneChange = vi.fn();
		render(
			<RecognitionControls
				{...base}
				microphones={[
					{
						deviceId: "builtin",
						label: "MacBook Pro Microphone",
						isDefault: true,
					},
					{ deviceId: "airpods", label: "AirPods Pro", isDefault: false },
				]}
				onMicrophoneChange={onMicrophoneChange}
			/>,
		);
		const select = screen.getAllByRole("combobox")[2] as HTMLSelectElement;
		const options = Array.from(select.querySelectorAll("option"));
		expect(options.map((option) => option.textContent)).toEqual([
			"System default",
			"MacBook Pro Microphone",
			"AirPods Pro",
		]);
		expect(select.value).toBe("");
		fireEvent.change(select, { target: { value: "builtin" } });
		expect(onMicrophoneChange).toHaveBeenCalledWith("builtin");
		fireEvent.change(select, { target: { value: "" } });
		expect(onMicrophoneChange).toHaveBeenCalledWith(null);
		// The hint that explains a silent default input is visible while default is chosen.
		expect(
			screen.getByText(/No input\? Pick the built-in microphone/),
		).toBeTruthy();
	});

	it("offers 'stop dictation after' only while wake is on", () => {
		const onWakeSilenceTimeoutChange = vi.fn();
		const props = { ...base, wakeAvailable: true, onWakeSilenceTimeoutChange };
		const { rerender } = render(
			<RecognitionControls {...props} wakeEnabled={false} />,
		);
		expect(screen.queryByText("Stop dictation after")).toBeNull();
		rerender(<RecognitionControls {...props} wakeEnabled />);
		const select = screen
			.getByText("Stop dictation after")
			.closest("label")
			?.querySelector("select") as HTMLSelectElement;
		expect(select.value).toBe("20000");
		expect(
			Array.from(select.querySelectorAll("option")).map((o) => o.textContent),
		).toEqual(["Never", "10 s", "20 s", "30 s", "60 s"]);
		fireEvent.change(select, { target: { value: "30000" } });
		expect(onWakeSilenceTimeoutChange).toHaveBeenCalledWith(30000);
	});

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
