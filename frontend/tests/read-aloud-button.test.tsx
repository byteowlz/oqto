import { ReadAloudButton } from "@/components/chat";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/hooks/use-tts", () => ({
	useTTSWithParagraphs: () => ({
		state: "idle",
		isSpeaking: false,
		isReading: true,
		isConnected: true,
		speak: vi.fn(),
		stop: vi.fn(),
		play: vi.fn(),
		previousParagraph: vi.fn(),
		nextParagraph: vi.fn(),
		currentParagraph: 0,
		totalParagraphs: 1,
		hasPrevious: false,
		hasNext: false,
		error: null,
		settings: { voice: "af_heart", speed: 1.3 },
		availableVoices: ["af_heart"],
		setVoice: vi.fn(),
		setSpeed: vi.fn(),
	}),
}));

describe("ReadAloudButton", () => {
	it("shows stop state when a paragraph sequence is active", () => {
		render(<ReadAloudButton text={"Hello world"} />);
		expect(
			screen.getAllByRole("button", { name: /stop/i }).length,
		).toBeGreaterThan(0);
		expect(screen.queryAllByRole("button", { name: /^read$/i })).toHaveLength(
			0,
		);
	});
});
