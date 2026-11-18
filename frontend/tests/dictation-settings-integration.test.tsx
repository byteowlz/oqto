import {
	DictationMicButton,
	DictationSettingsPanel,
} from "@/components/voice/ComposerDictation";
import { useDictation } from "@/features/voice/hooks/useDictation";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/voice/stt-service", () => ({
	STTService: vi.fn().mockImplementation(() => ({
		setCallbacks() {},
		async startListening() {},
		async finishListening() {},
		disconnect() {},
		setMicrophone() {},
		getInputVolume: () => 0,
		listMicrophones: async () => [
			{ deviceId: "builtin", label: "MacBook Pro Microphone", isDefault: true },
		],
	})),
}));
vi.mock("@/lib/control-plane-client", () => ({
	voiceProxyWsUrl: vi.fn(() => "wss://example.test/voice/stt"),
}));

let renders = 0;
function Composer() {
	renders++;
	const [open, setOpen] = useState(false);
	const dictation = useDictation({
		scopeKey: "s",
		config: null,
		onTranscript: () => {},
		onAutoSend: () => {},
	});
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

beforeEach(() => {
	renders = 0;
	const storage = new Map<string, string>();
	vi.mocked(localStorage.getItem).mockImplementation(
		(key) => storage.get(key) ?? null,
	);
	vi.mocked(localStorage.setItem).mockImplementation((key, value) => {
		storage.set(key, value);
	});
});

describe("dictation settings with the real hook", () => {
	it("auto-send can be switched on and the pause picker appears", async () => {
		render(<Composer />);
		await act(async () => {
			fireEvent.click(
				screen.getByRole("button", { name: "Dictation settings" }),
			);
		});
		const box = screen.getByRole("checkbox", { name: "Send by itself after" });
		expect(box).not.toBeChecked();
		await act(async () => {
			fireEvent.click(box);
		});
		expect(
			screen.getByRole("checkbox", { name: "Send by itself after" }),
		).toBeChecked();
		expect(screen.getByLabelText("Pause before sending")).toBeTruthy();
	});

	it("opening the panel does not re-render in a loop", async () => {
		render(<Composer />);
		await act(async () => {
			fireEvent.click(
				screen.getByRole("button", { name: "Dictation settings" }),
			);
		});
		const settled = renders;
		await act(async () => {
			await new Promise((resolve) => setTimeout(resolve, 200));
		});
		expect(renders - settled).toBeLessThan(5);
	});
});
