import { RecognitionControls } from "@/components/voice/RecognitionControls";
import { initI18n } from "@/lib/i18n";
import {
	DEFAULT_RECOGNITION_SETTINGS,
	parseRecognitionSettings,
} from "@/lib/voice/recognition-settings";
import { WakeMeter } from "@/lib/voice/wake-meter";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

initI18n();

const base = {
	remoteAvailable: false,
	onProviderChange: vi.fn(),
	onLanguageChange: vi.fn(),
	wakeAvailable: true,
	wakeEnabled: true,
	wakeState: "listening",
};

describe("wake debug settings", () => {
	it("persists only a threshold strictly between 0 and 1", () => {
		const parse = (wakeThreshold: unknown) =>
			parseRecognitionSettings(
				JSON.stringify({ wakeDebug: true, wakeThreshold }),
			);
		expect(parse(0.42)).toMatchObject({ wakeDebug: true, wakeThreshold: 0.42 });
		expect(parse(0)).toMatchObject({ wakeThreshold: null });
		expect(parse(1)).toMatchObject({ wakeThreshold: null });
		expect(parse("0.4")).toMatchObject({ wakeThreshold: null });
		expect(parseRecognitionSettings("{}")).toMatchObject({
			wakeDebug: false,
			wakeThreshold: null,
		});
	});

	it("offers the debug switch only once wake is on", () => {
		const meter = new WakeMeter();
		const props = {
			...base,
			recognition: DEFAULT_RECOGNITION_SETTINGS,
			wakeMeter: meter,
			onWakeDebugChange: vi.fn(),
		};
		const { rerender } = render(
			<RecognitionControls {...props} wakeEnabled={false} />,
		);
		expect(screen.queryByLabelText("Show wake score (debug)")).toBeNull();
		rerender(<RecognitionControls {...props} />);
		fireEvent.click(screen.getByLabelText("Show wake score (debug)"));
		expect(props.onWakeDebugChange).toHaveBeenCalledWith(true);
		expect(screen.queryByRole("meter")).toBeNull();
	});

	it("shows the live score against the threshold and the last trigger score", () => {
		const meter = new WakeMeter();
		render(
			<RecognitionControls
				{...base}
				recognition={{ ...DEFAULT_RECOGNITION_SETTINGS, wakeDebug: true }}
				wakeMeter={meter}
				onWakeDebugChange={vi.fn()}
				wakeReleaseThreshold={0.5}
				onWakeThresholdChange={vi.fn()}
			/>,
		);
		act(() => {
			vi.useFakeTimers();
			meter.push(0.31);
			vi.advanceTimersByTime(120);
			vi.useRealTimers();
		});
		expect(screen.getByRole("meter", { name: "Wake score" })).toHaveAttribute(
			"aria-valuenow",
			"0.31",
		);
		expect(screen.getByText(/Score 0.31 · 0.31/)).toBeTruthy();
		act(() => meter.trigger(0.87));
		expect(screen.getByText(/Last trigger 0.87/)).toBeTruthy();
	});

	it("moves the threshold with the slider and restores the release value", () => {
		const onWakeThresholdChange = vi.fn();
		const props = {
			...base,
			wakeMeter: new WakeMeter(),
			onWakeDebugChange: vi.fn(),
			wakeReleaseThreshold: 0.5,
			onWakeThresholdChange,
		};
		const { rerender } = render(
			<RecognitionControls
				{...props}
				recognition={{ ...DEFAULT_RECOGNITION_SETTINGS, wakeDebug: true }}
			/>,
		);
		const slider = screen.getByLabelText("Wake threshold") as HTMLInputElement;
		expect(slider.value).toBe("0.5");
		expect(screen.queryByText(/Use release threshold/)).toBeNull();
		fireEvent.change(slider, { target: { value: "0.35" } });
		expect(onWakeThresholdChange).toHaveBeenCalledWith(0.35);
		rerender(
			<RecognitionControls
				{...props}
				recognition={{
					...DEFAULT_RECOGNITION_SETTINGS,
					wakeDebug: true,
					wakeThreshold: 0.35,
				}}
			/>,
		);
		expect(screen.getByText(/Threshold 0.35 \(custom\)/)).toBeTruthy();
		fireEvent.click(screen.getByText(/Use release threshold/));
		expect(onWakeThresholdChange).toHaveBeenLastCalledWith(null);
	});
});
