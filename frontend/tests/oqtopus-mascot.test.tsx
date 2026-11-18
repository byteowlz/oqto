import { DictationMicButton } from "@/components/voice/ComposerDictation";
import {
	OQTOPUS_REACTIONS,
	OqtopusMascot,
	resetOqtopusAvailabilityForTests,
} from "@/components/voice/OqtopusMascot";
import type { UseDictationReturn } from "@/features/voice/hooks/useDictation";
import { DEFAULT_RECOGNITION_SETTINGS } from "@/lib/voice/recognition-settings";
import { WakeMeter } from "@/lib/voice/wake-meter";
import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

function fakePlayer() {
	return { play: vi.fn(), setIdle: vi.fn(), setTheme: vi.fn() };
}
function attach(player: ReturnType<typeof fakePlayer>) {
	const frame = document.querySelector("iframe") as HTMLIFrameElement;
	(frame.contentWindow as Window & { oqtopus?: unknown }).oqtopus = player;
	return frame;
}

beforeEach(() => {
	resetOqtopusAvailabilityForTests();
	vi.mocked(fetch).mockReset();
});
afterEach(() => vi.useRealTimers());

describe("Oqtopus mascot", () => {
	it("embeds the staged vector player through its host page, decoratively", () => {
		render(<OqtopusMascot lively={false} cue={null} theme="dark" />);
		const frame = document.querySelector("iframe") as HTMLIFrameElement;
		expect(frame.getAttribute("src")).toMatch(
			/oqtopus\/embed\.html\?.*idle=0.*theme=dark/,
		);
		expect(frame).toHaveAttribute("aria-hidden", "true");
		expect(frame).toHaveAttribute("tabindex", "-1");
	});

	it("waits for the player API, then applies idle, theme and a fresh reaction once", async () => {
		vi.useFakeTimers();
		const player = fakePlayer();
		const cue = { kind: "wake" as const, at: Date.now() };
		const { rerender } = render(
			<OqtopusMascot lively={false} cue={cue} theme="light" />,
		);
		// Player not ready yet: nothing is lost, it is applied once published.
		attach(player);
		act(() => vi.advanceTimersByTime(200));
		expect(player.setTheme).toHaveBeenLastCalledWith("light");
		expect(player.setIdle).toHaveBeenLastCalledWith(false);
		expect(player.play).toHaveBeenCalledExactlyOnceWith(OQTOPUS_REACTIONS.wake);
		// Re-renders with the same cue do not replay it; dictation makes it lively.
		rerender(<OqtopusMascot lively cue={cue} theme="light" />);
		expect(player.play).toHaveBeenCalledOnce();
		expect(player.setIdle).toHaveBeenLastCalledWith(true);
		rerender(
			<OqtopusMascot
				lively
				cue={{ kind: "sent", at: Date.now() }}
				theme="light"
			/>,
		);
		expect(player.play).toHaveBeenLastCalledWith("hat-tip");
	});

	it("keeps the newest desired state during loading and reapplies theme-only changes", () => {
		vi.useFakeTimers();
		const player = fakePlayer();
		const cue = { kind: "sent" as const, at: Date.now() + 1 };
		const { rerender } = render(
			<OqtopusMascot lively={false} cue={null} theme="dark" />,
		);
		rerender(<OqtopusMascot lively cue={cue} theme="light" />);
		attach(player);
		act(() => vi.advanceTimersByTime(200));
		expect(player.setIdle).toHaveBeenLastCalledWith(true);
		expect(player.setTheme).toHaveBeenLastCalledWith("light");
		expect(player.play).toHaveBeenCalledExactlyOnceWith("hat-tip");
		rerender(<OqtopusMascot lively cue={cue} theme="dark" />);
		// Theme is also in the embed URL: its replacement page publishes anew.
		attach(player);
		act(() => vi.advanceTimersByTime(200));
		expect(player.setTheme).toHaveBeenLastCalledWith("dark");
		expect(player.play).toHaveBeenCalledOnce();
	});

	it("skips a reaction that is stale by the time the player is ready", () => {
		vi.useFakeTimers();
		const player = fakePlayer();
		render(
			<OqtopusMascot
				lively={false}
				cue={{ kind: "error", at: Date.now() - 10_000 }}
				theme="dark"
			/>,
		);
		attach(player);
		act(() => vi.advanceTimersByTime(200));
		expect(player.play).not.toHaveBeenCalled();
	});
});

function dictation(
	patch: Partial<UseDictationReturn> = {},
): UseDictationReturn {
	const noop = () => {};
	return {
		isActive: false,
		liveTranscript: "",
		vadProgress: 0,
		getInputVolume: () => 0,
		isConnected: false,
		error: null,
		dismissError: noop,
		preparation: null,
		download: null,
		recognition: { ...DEFAULT_RECOGNITION_SETTINGS, wakeEnabled: true },
		remoteAvailable: false,
		setRecognitionProvider: noop,
		microphones: [],
		refreshMicrophones: async () => {},
		setWakeSilenceTimeoutMs: noop,
		setWakeSound: noop,
		setSendSound: noop,
		sendCountdown: null,
		voiceCue: null,
		setMicrophone: noop,
		setRecognitionLanguage: noop,
		autoSendEnabled: false,
		autoSendPauseMs: 2000,
		setAutoSendEnabled: noop,
		setAutoSendPauseMs: noop,
		start: async () => {},
		stop: noop,
		cancel: noop,
		wakeAvailable: true,
		wakeState: "listening",
		wakePhrases: ["hey_oqto"],
		setWakeEnabled: noop,
		setWakePhrase: noop,
		setWakeDebug: noop,
		setWakeThreshold: noop,
		wakeReleaseThreshold: 0.5,
		wakeMeter: new WakeMeter(),
		wakeTriggerScore: null,
		setWakePlayback: noop,
		...patch,
	};
}

describe("mic button with the mascot", () => {
	it("shows the mascot instead of the icon and dot when staged and wake is on", async () => {
		vi.mocked(fetch).mockResolvedValue(new Response(null, { status: 200 }));
		render(<DictationMicButton dictation={dictation()} />);
		const button = screen.getByRole("button", { name: "Dictation settings" });
		await waitFor(() => expect(button).toHaveAttribute("data-mascot"));
		expect(button.querySelector("iframe")).toBeTruthy();
		expect(button.querySelector(".composer-dictation-mic__wake")).toBeNull();
		expect(button).toHaveAccessibleDescription('Listening for "hey oqto"');
	});

	it("keeps the plain icon when the mascot was not staged", async () => {
		vi.mocked(fetch).mockResolvedValue(new Response(null, { status: 404 }));
		render(<DictationMicButton dictation={dictation()} />);
		const button = screen.getByRole("button", { name: "Dictation settings" });
		await act(async () => {
			await Promise.resolve();
		});
		expect(button).not.toHaveAttribute("data-mascot");
		expect(button.querySelector("iframe")).toBeNull();
		expect(button.querySelector(".composer-dictation-mic__wake")).toBeTruthy();
	});

	it("never loads the mascot while wake is off", () => {
		render(
			<DictationMicButton
				dictation={dictation({
					recognition: DEFAULT_RECOGNITION_SETTINGS,
					wakeState: "off",
				})}
			/>,
		);
		expect(fetch).not.toHaveBeenCalledWith(
			expect.stringContaining("oqtopus"),
			expect.anything(),
		);
	});
});
