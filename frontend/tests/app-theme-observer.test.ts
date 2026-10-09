import {
	detectAppColorScheme,
	observeAppTheme,
} from "@/features/sessions/components/app-theme-observer";
import { afterEach, describe, expect, it, vi } from "vitest";

const root = document.documentElement;

afterEach(() => {
	root.classList.remove("dark", "light");
	root.removeAttribute("data-theme");
	root.style.removeProperty("color-scheme");
	vi.unstubAllGlobals();
});

function mockPreference(matches: boolean) {
	const listeners = new Set<() => void>();
	const preference = {
		matches,
		addEventListener: vi.fn((_type: string, listener: () => void) => {
			listeners.add(listener);
		}),
		removeEventListener: vi.fn((_type: string, listener: () => void) => {
			listeners.delete(listener);
		}),
	};
	vi.stubGlobal(
		"matchMedia",
		vi.fn(() => preference),
	);
	return { preference, listeners };
}

describe("App theme host observer", () => {
	it("honors explicit mode over system preference", () => {
		mockPreference(true);
		root.classList.add("light");
		expect(detectAppColorScheme(root)).toBe("light");
		root.classList.replace("light", "dark");
		expect(detectAppColorScheme(root)).toBe("dark");
		root.classList.remove("dark");
		root.dataset.theme = "light";
		expect(detectAppColorScheme(root)).toBe("light");
	});

	it("treats 'light dark' as ambiguous and follows the OS", () => {
		const { preference } = mockPreference(false);
		root.style.colorScheme = "light dark";
		expect(detectAppColorScheme(root)).toBe("light");
		preference.matches = true;
		expect(detectAppColorScheme(root)).toBe("dark");
		root.style.colorScheme = "light";
		expect(detectAppColorScheme(root)).toBe("light");
	});

	it("notifies on OS and root changes and cleans up both subscriptions", async () => {
		const { preference, listeners } = mockPreference(false);
		const notify = vi.fn();
		const stop = observeAppTheme(root, notify);
		expect(listeners.size).toBe(1);
		for (const listener of listeners) listener();
		expect(notify).toHaveBeenCalledTimes(1);
		root.classList.add("dark");
		await vi.waitFor(() => expect(notify).toHaveBeenCalledTimes(2));
		stop();
		expect(preference.removeEventListener).toHaveBeenCalledWith(
			"change",
			notify,
		);
		expect(listeners.size).toBe(0);
		root.classList.remove("dark");
		await Promise.resolve();
		expect(notify).toHaveBeenCalledTimes(2);
	});
});
