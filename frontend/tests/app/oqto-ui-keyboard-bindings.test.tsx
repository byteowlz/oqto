import { KeyboardBindings } from "@/src/oqto-ui/app/KeyboardBindings";
import { captureChord } from "@/src/oqto-ui/app/chords";
import { bindingsFromConfig } from "@/src/oqto-ui/compositor/react/config-bindings";
import { DEFAULT_KEY_BINDINGS } from "@/src/oqto-ui/compositor/react/keybindings";
import type { BindingOverride } from "@/src/oqto-ui/platform/binding-storage";
import {
	bindingStorageKey,
	readBindingOverrides,
	writeBindingOverrides,
} from "@/src/oqto-ui/platform/binding-storage";
import { memoryLayoutStorage } from "@/src/oqto-ui/platform/layout-storage";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { initI18n } from "../../lib/i18n";

initI18n();

const TOGGLE = "compositor.toggleNavigation";

function chordFor(bindings: ReturnType<typeof bindingsFromConfig>, id: string) {
	return bindings.find(
		(binding) =>
			(binding.action.type === "toggle-navigation" ? TOGGLE : "") === id,
	)?.chord;
}

describe("binding layers", () => {
	it("puts the device layer over the config, which is over the defaults", () => {
		const configured = bindingsFromConfig([
			{ keys: "ctrl+alt+b", action: TOGGLE },
		]);
		expect(chordFor(configured, TOGGLE)).toBe("Ctrl+Alt+b");
		const device = bindingsFromConfig(
			[{ keys: "alt+shift+t", action: TOGGLE }],
			configured,
		);
		expect(chordFor(device, TOGGLE)).toBe("Alt+Shift+t");
		// One action holds one chord: the layer below is released, not kept.
		expect(
			device.filter((binding) => binding.action.type === "toggle-navigation"),
		).toHaveLength(1);
	});

	it("survives unreadable storage and round-trips what it wrote", () => {
		const storage = memoryLayoutStorage();
		const key = bindingStorageKey("live");
		expect(readBindingOverrides(storage, key)).toEqual([]);
		storage.write(key, "not json");
		expect(readBindingOverrides(storage, key)).toEqual([]);
		const overrides: BindingOverride[] = [
			{ keys: "alt+shift+t", action: TOGGLE },
		];
		writeBindingOverrides(storage, key, overrides);
		expect(readBindingOverrides(storage, key)).toEqual(overrides);
	});

	it("reads a chord only from a real combination", () => {
		const press = (init: Partial<KeyboardEvent>) =>
			captureChord({
				key: "b",
				ctrlKey: false,
				altKey: false,
				metaKey: false,
				shiftKey: false,
				...init,
			} as never);
		expect(press({ altKey: true })).toBe("alt+b");
		expect(press({ ctrlKey: true, shiftKey: true })).toBe("ctrl+shift+b");
		expect(press({ key: "ArrowLeft", altKey: true })).toBe("alt+left");
		// A bare letter would swallow typing, and a modifier alone is not a chord.
		expect(press({})).toBeNull();
		expect(press({ key: "Shift", shiftKey: true })).toBeNull();
	});
});

describe("keyboard settings", () => {
	it("lists every action and records a new chord on the row", () => {
		const onChange = vi.fn();
		render(
			<KeyboardBindings
				bindings={bindingsFromConfig([])}
				overrides={[]}
				onChange={onChange}
			/>,
		);
		expect(screen.getAllByRole("listitem")).toHaveLength(
			DEFAULT_KEY_BINDINGS.length,
		);
		const row = screen.getByText("Toggle navigation sidebar")
			.parentElement as HTMLElement;
		const button = row.querySelector("button") as HTMLElement;
		expect(button).toHaveTextContent("Alt+b");
		fireEvent.click(button);
		expect(button).toHaveTextContent("Press keys…");
		fireEvent.keyDown(button, { key: "t", altKey: true, shiftKey: true });
		expect(onChange).toHaveBeenCalledWith([
			{ keys: "alt+shift+t", action: TOGGLE },
		]);
	});

	it("releases a chord taken from another action", () => {
		const onChange = vi.fn();
		render(
			<KeyboardBindings
				bindings={bindingsFromConfig([])}
				overrides={[{ keys: "alt+shift+t", action: "compositor.undo" }]}
				onChange={onChange}
			/>,
		);
		const row = screen.getByText("Toggle navigation sidebar")
			.parentElement as HTMLElement;
		const button = row.querySelector("button") as HTMLElement;
		fireEvent.click(button);
		fireEvent.keyDown(button, { key: "t", altKey: true, shiftKey: true });
		expect(onChange).toHaveBeenCalledWith([
			{ keys: "alt+shift+t", action: TOGGLE },
		]);
	});

	it("offers a reset only for an overridden action", () => {
		const onChange = vi.fn();
		render(
			<KeyboardBindings
				bindings={bindingsFromConfig([{ keys: "alt+shift+t", action: TOGGLE }])}
				overrides={[{ keys: "alt+shift+t", action: TOGGLE }]}
				onChange={onChange}
			/>,
		);
		const resets = screen.getAllByRole("button", { name: "Reset to default" });
		expect(resets).toHaveLength(1);
		fireEvent.click(resets[0]);
		expect(onChange).toHaveBeenCalledWith([]);
	});
});
