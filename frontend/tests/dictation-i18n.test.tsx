import {
	DictationMicButton,
	DictationSettingsPanel,
} from "@/components/voice/ComposerDictation";
import { DictationOverlay } from "@/components/voice/DictationOverlay";
import { RecognitionControls } from "@/components/voice/RecognitionControls";
import type { UseDictationReturn } from "@/features/voice/hooks/useDictation";
import { i18n } from "@/lib/i18n";
import { DEFAULT_RECOGNITION_SETTINGS } from "@/lib/voice/recognition-settings";
import { WakeMeter } from "@/lib/voice/wake-meter";
import de from "@/messages/de.json";
import en from "@/messages/en.json";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

const wake = {
	recognition: {
		...DEFAULT_RECOGNITION_SETTINGS,
		wakeEnabled: true,
		wakeDebug: true,
	},
	remoteAvailable: false,
	onProviderChange: vi.fn(),
	onLanguageChange: vi.fn(),
	wakeAvailable: true,
	wakeEnabled: true,
	wakePhrases: ["hey_oqto", "oqto"],
	wakePhrase: "hey_oqto",
	onWakePhraseChange: vi.fn(),
	wakeState: "listening",
	wakeMeter: new WakeMeter(),
	onWakeDebugChange: vi.fn(),
	wakeReleaseThreshold: 0.5,
	onWakeThresholdChange: vi.fn(),
};

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

afterEach(async () => {
	await i18n.changeLanguage("en");
});

describe("dictation settings in German", () => {
	it("translates every dictation settings key in both dictionaries", () => {
		const keys = (d: { voice: { dictationSettings: object } }) =>
			Object.keys(d.voice.dictationSettings).sort();
		expect(keys(de)).toEqual(keys(en));
		// Product and protocol names stay as they are in both languages.
		const identical = new Set([
			"providerRemote",
			"wakePhraseLabel",
			"wakeMeterLabel",
			"wakeSliderLabel",
			"messageLabel",
			"wakeThresholdRelease",
			"wakeThresholdCustom",
			"wakeSecondsShort",
			"seconds",
			"providerBadgeRemote",
			"wakePhraseLabel",
		]);
		for (const [name, value] of Object.entries(de.voice.dictationSettings))
			if (!identical.has(name))
				expect(value, name).not.toBe(
					(en.voice.dictationSettings as Record<string, string>)[name],
				);
	});

	it("renders the settings and the debug panel in German", async () => {
		await act(() => i18n.changeLanguage("de"));
		render(<RecognitionControls {...wake} />);
		expect(screen.getByText("Erkennung")).toBeTruthy();
		expect(screen.getByText("Sprache")).toBeTruthy();
		expect(screen.getByText("Diktat per Weckwort starten")).toBeTruthy();
		expect(screen.getByText(/Hört lokal zu/)).toBeTruthy();
		expect(screen.getByLabelText("Weckwort")).toBeTruthy();
		expect(screen.getByText("Weckwert anzeigen (Debug)")).toBeTruthy();
		expect(screen.getByRole("meter", { name: "Weckwert" })).toBeTruthy();
		expect(screen.getByLabelText("Weckwort-Schwelle")).toBeTruthy();
		expect(screen.getByText(/Schwelle 0.50 \(Release\)/)).toBeTruthy();
	});

	it("renders the settings overlay in German", async () => {
		await act(() => i18n.changeLanguage("de"));
		const dictation = {
			isActive: false,
			liveTranscript: "",
			vadProgress: 0,
			getInputVolume: () => 0,
			isConnected: true,
			error: null,
			dismissError: vi.fn(),
			preparation: null,
			download: null,
			recognition: wake.recognition,
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
			wakeAvailable: true,
			wakeState: "listening",
			wakePhrases: ["hey_oqto"],
			setWakeEnabled: vi.fn(),
			setWakePhrase: vi.fn(),
			setWakeDebug: vi.fn(),
			setWakeThreshold: vi.fn(),
			wakeReleaseThreshold: 0.5,
			wakeMeter: new WakeMeter(),
			wakeTriggerScore: null,
			setWakePlayback: vi.fn(),
		} satisfies UseDictationReturn;
		render(<ComposerHarness dictation={dictation} />);
		const button = screen.getByRole("button", { name: "Diktiereinstellungen" });
		expect(button).toHaveAccessibleDescription("Hört auf „hey oqto“");
		fireEvent.click(button);
		expect(screen.getByText("Automatisch senden nach")).toBeTruthy();
		expect(screen.getByText("Sendet nicht von selbst")).toBeTruthy();
		expect(
			screen.getByRole("button", { name: "Diktieren starten" }),
		).toBeTruthy();
	});

	it("renders the dictation overlay in German", async () => {
		await act(() => i18n.changeLanguage("de"));
		render(
			<DictationOverlay
				open
				preparation={null}
				value=""
				liveTranscript=""
				placeholder=""
				vadProgress={0}
				autoSend={false}
				onAutoSendChange={vi.fn()}
				onStop={vi.fn()}
				onChange={vi.fn()}
				onKeyDown={vi.fn()}
				onPaste={vi.fn()}
				onFocus={vi.fn()}
				onBlur={vi.fn()}
			/>,
		);
		expect(screen.getByLabelText("Diktieren abbrechen")).toBeTruthy();
		expect(screen.getByLabelText("Nachricht")).toBeTruthy();
		expect(screen.getByText("Warte auf Sprache…")).toBeTruthy();
	});
});
