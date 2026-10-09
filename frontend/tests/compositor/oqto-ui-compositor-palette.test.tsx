import type { ContentRef } from "@/src/oqto-ui/compositor/index";
import { CompositorHost } from "@/src/oqto-ui/compositor/react/CompositorHost";
import {
	bindingsFromConfig,
	normalizeConfigChord,
} from "@/src/oqto-ui/compositor/react/config-bindings";
import {
	DEFAULT_KEY_BINDINGS,
	actionFromId,
	actionId,
} from "@/src/oqto-ui/compositor/react/keybindings";
import { createCompositorStore } from "@/src/oqto-ui/compositor/react/store";
import { DEFAULT_OQTO_UI_CONFIG } from "@/src/oqto-ui/platform/contracts";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
	LABELS,
	VIEWPORT,
	activeArrangementOf,
	classicLayout,
	containerByRole,
} from "./fixtures";

const DESKTOP = { ...VIEWPORT, responsive: "scroll" as const };

function mount(keyBindings = DEFAULT_KEY_BINDINGS) {
	const store = createCompositorStore(classicLayout());
	const renderContent = (content: ContentRef) => (
		<output data-testid={`content-${content.id}`}>{content.id}</output>
	);
	const view = render(
		<CompositorHost
			store={store}
			viewport={DESKTOP}
			renderContent={renderContent}
			contentLabel={(c) => c.id}
			labels={LABELS}
			keyBindings={keyBindings}
		/>,
	);
	return {
		store,
		view,
		root: view.container.querySelector(".oqto-compositor") as HTMLElement,
	};
}

describe("config-layer bindings (ADR-0040 data)", () => {
	it("normalizes config chords into the adapter's chord syntax", () => {
		expect(normalizeConfigChord("ctrl+shift+p")).toBe("Ctrl+Shift+p");
		expect(normalizeConfigChord("alt+right")).toBe("Alt+ArrowRight");
		expect(normalizeConfigChord("Shift+Alt+Left")).toBe("Alt+Shift+ArrowLeft");
		expect(normalizeConfigChord("super+b")).toBeNull();
		expect(normalizeConfigChord("")).toBeNull();
	});

	it("overrides defaults by action id and ignores unknown actions", () => {
		const bindings = bindingsFromConfig([
			{ keys: "ctrl+alt+b", action: "compositor.toggleNavigation" },
			{ keys: "ctrl+k", action: "compositor.doesNotExist" },
			{ keys: "ctrl+shift+f", action: "view.openFiles" },
		]);
		expect(
			bindings.filter(
				(b) => actionId(b.action) === "compositor.toggleNavigation",
			),
		).toEqual([{ chord: "Ctrl+Alt+b", action: { type: "toggle-navigation" } }]);
		expect(bindings.some((b) => b.chord === "Alt+b")).toBe(false);
		expect(bindings).toHaveLength(DEFAULT_KEY_BINDINGS.length);
		expect(actionFromId("compositor.focus.end")).toEqual({
			type: "focus-neighbor",
			direction: "inline-end",
		});
		expect(actionFromId("nope")).toBeNull();
	});

	it("the dist default config is the readable source of the same bindings", () => {
		// ADR-0040: the shipped preset must express what the adapter does by
		// default, so merging it over the defaults changes nothing.
		const merged = bindingsFromConfig(DEFAULT_OQTO_UI_CONFIG.config.bindings);
		const asMap = (bindings: readonly { chord: string; action: object }[]) =>
			new Map(bindings.map((b) => [actionId(b.action as never), b.chord]));
		expect(asMap(merged)).toEqual(asMap(DEFAULT_KEY_BINDINGS));
		const compositorEntries = DEFAULT_OQTO_UI_CONFIG.config.bindings.filter(
			(b) => actionFromId(b.action) !== null,
		);
		expect(compositorEntries).toHaveLength(DEFAULT_KEY_BINDINGS.length);
	});

	it("every default action has a stable id that round-trips", () => {
		for (const binding of DEFAULT_KEY_BINDINGS) {
			expect(actionFromId(actionId(binding.action))).toEqual(binding.action);
		}
	});
});

describe("command palette", () => {
	it("opens on the configured chord, lists actions with their chords, filters, and runs", () => {
		const { store, root } = mount();
		fireEvent.keyDown(root, { key: "P", ctrlKey: true, shiftKey: true });
		const dialog = screen.getByRole("dialog", { name: LABELS.palette.title });
		expect(dialog).toBeInTheDocument();
		expect(screen.getByText("Alt+b")).toBeInTheDocument();
		fireEvent.change(screen.getByLabelText(LABELS.palette.searchPlaceholder), {
			target: { value: "navigation" },
		});
		expect(screen.getAllByRole("option")).toHaveLength(1);
		fireEvent.keyDown(screen.getByLabelText(LABELS.palette.searchPlaceholder), {
			key: "Enter",
		});
		expect(screen.queryByRole("dialog")).toBeNull();
		expect(containerByRole(store.getSnapshot(), "navigation").collapsed).toBe(
			true,
		);
	});

	it("closes on Escape without running anything and shows an empty state", () => {
		const { store, root } = mount();
		fireEvent.keyDown(root, { key: "P", ctrlKey: true, shiftKey: true });
		fireEvent.change(screen.getByLabelText(LABELS.palette.searchPlaceholder), {
			target: { value: "zzz" },
		});
		expect(screen.getByText(LABELS.palette.noMatches)).toBeInTheDocument();
		fireEvent.keyDown(screen.getByLabelText(LABELS.palette.searchPlaceholder), {
			key: "Escape",
		});
		expect(screen.queryByRole("dialog")).toBeNull();
		expect(store.getSnapshot().revision).toBe(0);
	});

	it("reflects config-overridden chords in its help", () => {
		const bindings = bindingsFromConfig([
			{ keys: "ctrl+alt+b", action: "compositor.toggleNavigation" },
		]);
		const { root } = mount(bindings);
		fireEvent.keyDown(root, { key: "P", ctrlKey: true, shiftKey: true });
		expect(screen.getByText("Ctrl+Alt+b")).toBeInTheDocument();
		expect(screen.queryByText("Alt+b")).toBeNull();
	});
});

describe("live resize preview", () => {
	it("previews the pending resize while dragging and commits once on release", () => {
		const { store, view } = mount();
		const gutter = view.getAllByLabelText(LABELS.resizeColumns)[1];
		gutter.setPointerCapture = () => {};
		const grid = () =>
			(
				view.container.querySelector(".oqto-compositor-grid") as HTMLElement
			).style.getPropertyValue("--oqto-compositor-columns");
		const before = grid();
		fireEvent.pointerDown(gutter, { clientX: 1000, pointerId: 1 });
		fireEvent.pointerMove(gutter, { clientX: 1100, pointerId: 1 });
		expect(grid()).not.toBe(before);
		expect(view.container.querySelector(".oqto-compositor")).toHaveAttribute(
			"data-previewing",
		);
		// Nothing committed yet: canonical state and revision untouched.
		expect(store.getSnapshot().revision).toBe(0);
		expect(
			activeArrangementOf(store.getSnapshot()).grid.columns[1].size.value,
		).toBe(2);
		fireEvent.pointerUp(gutter, { clientX: 1100, pointerId: 1 });
		expect(store.getSnapshot().revision).toBe(1);
		expect(
			activeArrangementOf(store.getSnapshot()).grid.columns[1].size.value,
		).toBeGreaterThan(2);
		expect(
			view.container.querySelector(".oqto-compositor"),
		).not.toHaveAttribute("data-previewing");
		expect(grid()).not.toBe(before);
	});
});
