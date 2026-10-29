import { ChatDetail } from "@/src/oqto-ui/chat/ChatDetail";
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "../../lib/i18n";

initI18n();

/**
 * The suite stubs localStorage with mock functions; the setting is a real
 * host store, so give it one that remembers.
 */
let stored: { [key: string]: string } = {};

beforeEach(() => {
	stored = {};
	vi.mocked(window.localStorage.getItem).mockImplementation(
		(key: string) => stored[key] ?? null,
	);
	vi.mocked(window.localStorage.setItem).mockImplementation(
		(key: string, value: string) => {
			stored[key] = value;
		},
	);
});

describe("chat detail", () => {
	it("offers three levels named for what the reader sees", () => {
		render(<ChatDetail />);
		expect(
			screen
				.getAllByRole("radio")
				.map((option) => option.closest("label")?.textContent),
		).toEqual([
			"AnswersWhat the agent said, with its work folded away",
			'SummaryAnswers plus folded steps, one "N steps" line you can open',
			"EverythingEvery tool call and thought, expanded",
		]);
	});

	it("starts at Summary: collapsed activity, like Desktop and Mobile", () => {
		render(<ChatDetail />);
		expect(screen.getByRole("radio", { name: /^Summary/ })).toBeChecked();
	});

	it("writes the host-wide setting the shared renderer reads", () => {
		render(<ChatDetail />);
		fireEvent.click(screen.getByRole("radio", { name: /^Answers/ }));
		expect(stored["oqto:chatVerbosity"]).toBe("1");
		expect(screen.getByRole("radio", { name: /^Answers/ })).toBeChecked();
		fireEvent.click(screen.getByRole("radio", { name: /^Summary/ }));
		expect(stored["oqto:chatVerbosity"]).toBe("2");
	});

	it("is one named group of native radios", () => {
		render(<ChatDetail />);
		const group = screen.getByRole("group", { name: "Chat detail" });
		const radios = screen.getAllByRole("radio");
		expect(radios).toHaveLength(3);
		for (const radio of radios) {
			expect(group).toContainElement(radio);
			expect(radio.tagName).toBe("INPUT");
		}
		expect(
			new Set(radios.map((radio) => radio.getAttribute("name"))).size,
		).toBe(1);
	});

	it("shows the level the host already had", () => {
		stored["oqto:chatVerbosity"] = "1";
		render(<ChatDetail />);
		expect(screen.getByRole("radio", { name: /^Answers/ })).toBeChecked();
	});
});
