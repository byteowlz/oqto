import type { ContentRef } from "@/src/oqto-ui/compositor/index";
import {
	applyTransaction,
	solveLayoutGeometry,
} from "@/src/oqto-ui/compositor/index";
import { neighborContainer } from "@/src/oqto-ui/compositor/navigation";
import { CompositorHost } from "@/src/oqto-ui/compositor/react/CompositorHost";
import {
	DEFAULT_KEY_BINDINGS,
	matchBinding,
	resolveAction,
} from "@/src/oqto-ui/compositor/react/keybindings";
import { createCompositorStore } from "@/src/oqto-ui/compositor/react/store";
import { act, fireEvent, render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
	LABELS,
	VIEWPORT,
	activeArrangementOf,
	chatContent,
	classicLayout,
	containerByRole,
	filesContent,
	gitContent,
	sessionsContent,
} from "./fixtures";

const DESKTOP = { ...VIEWPORT, responsive: "scroll" as const };
const chord = (
	key: string,
	mods: Partial<{
		altKey: boolean;
		shiftKey: boolean;
		ctrlKey: boolean;
		metaKey: boolean;
	}> = {},
) => ({
	key,
	altKey: false,
	ctrlKey: false,
	metaKey: false,
	shiftKey: false,
	...mods,
});

function withBlockSplit() {
	const layout = classicLayout();
	const result = applyTransaction(layout, {
		expectedRevision: 0,
		commands: [
			{
				type: "split",
				contentId: filesContent.id,
				relativeTo: containerByRole(layout, "primary").id,
				edge: "block-end",
			},
		],
	});
	if (!result.ok) throw new Error("setup failed");
	return result.snapshot;
}

describe("kernel directional navigation", () => {
	it("resolves inline neighbors across the classic preset and stops at the edges", () => {
		const layout = classicLayout();
		const arrangement = activeArrangementOf(layout);
		const nav = containerByRole(layout, "navigation").id;
		const primary = containerByRole(layout, "primary").id;
		const aux = containerByRole(layout, "auxiliary").id;
		expect(
			neighborContainer(arrangement, layout.containers, nav, "inline-end"),
		).toBe(primary);
		expect(
			neighborContainer(arrangement, layout.containers, primary, "inline-end"),
		).toBe(aux);
		expect(
			neighborContainer(arrangement, layout.containers, aux, "inline-end"),
		).toBeNull();
		expect(
			neighborContainer(
				arrangement,
				layout.containers,
				primary,
				"inline-start",
			),
		).toBe(nav);
		expect(
			neighborContainer(arrangement, layout.containers, primary, "block-end"),
		).toBeNull();
	});

	it("prefers the largest cross-axis overlap and skips collapsed Containers", () => {
		const layout = withBlockSplit();
		const arrangement = activeArrangementOf(layout);
		const nav = containerByRole(layout, "navigation").id;
		const primary = containerByRole(layout, "primary").id;
		const below = arrangement.grid.placements.find(
			(p) => p.row === 1 && p.column === 1,
		)?.containerId as never;
		expect(
			neighborContainer(arrangement, layout.containers, primary, "block-end"),
		).toBe(below);
		expect(
			neighborContainer(arrangement, layout.containers, below, "block-start"),
		).toBe(primary);
		// The full-height nav spans both rows; from below, left is still nav.
		expect(
			neighborContainer(arrangement, layout.containers, below, "inline-start"),
		).toBe(nav);
		const collapsedNav = layout.containers.map((c) =>
			c.id === nav ? { ...c, collapsed: true } : c,
		);
		expect(
			neighborContainer(arrangement, collapsedNav, primary, "inline-start"),
		).toBeNull();
	});
});

describe("bindings as data", () => {
	it("matches chords including shifted punctuation", () => {
		expect(
			matchBinding(chord("ArrowRight", { altKey: true }), DEFAULT_KEY_BINDINGS),
		).toEqual({ type: "focus-neighbor", direction: "inline-end" });
		expect(
			matchBinding(
				chord("ArrowRight", { altKey: true, shiftKey: true }),
				DEFAULT_KEY_BINDINGS,
			),
		).toEqual({ type: "move-to-neighbor", direction: "inline-end" });
		expect(
			matchBinding(
				chord(">", { altKey: true, shiftKey: true }),
				DEFAULT_KEY_BINDINGS,
			),
		).toEqual({ type: "scroll", delta: 1 });
		expect(
			matchBinding(chord("B", { altKey: true }), DEFAULT_KEY_BINDINGS),
		).toEqual({ type: "toggle-navigation" });
		expect(matchBinding(chord("ArrowRight"), DEFAULT_KEY_BINDINGS)).toBeNull();
		expect(
			matchBinding(
				chord("ArrowRight", { ctrlKey: true }),
				DEFAULT_KEY_BINDINGS,
			),
		).toBeNull();
	});

	it("resolves actions to semantic commands against the canonical snapshot", () => {
		const layout = classicLayout();
		const geometry = solveLayoutGeometry(layout, VIEWPORT);
		expect(
			resolveAction(layout, geometry, {
				type: "focus-neighbor",
				direction: "inline-end",
			}),
		).toEqual([{ type: "focus", contentId: filesContent.id }]);
		expect(
			resolveAction(layout, geometry, {
				type: "focus-neighbor",
				direction: "block-end",
			}),
		).toEqual([]);
		expect(
			resolveAction(layout, geometry, {
				type: "move-to-neighbor",
				direction: "inline-start",
			}),
		).toEqual([
			{
				type: "move",
				contentId: chatContent.id,
				destination: { containerId: containerByRole(layout, "navigation").id },
			},
		]);
		expect(
			resolveAction(layout, geometry, {
				type: "move-to-neighbor",
				direction: "block-end",
			}),
		).toEqual([
			{ type: "split", contentId: chatContent.id, edge: "block-end" },
		]);
		expect(
			resolveAction(layout, geometry, { type: "toggle-navigation" }),
		).toEqual([
			{
				type: "collapse",
				containerId: containerByRole(layout, "navigation").id,
				collapsed: true,
			},
		]);
		expect(resolveAction(layout, geometry, { type: "close-focused" })).toEqual([
			{ type: "close", contentId: chatContent.id },
		]);
		expect(
			resolveAction(layout, geometry, { type: "cycle-tab", delta: 1 }),
		).toEqual([]);
		const resize = resolveAction(layout, geometry, {
			type: "resize-focused",
			deltaPx: 40,
		});
		expect(resize).toHaveLength(2);
		expect(resolveAction(layout, geometry, { type: "undo" })).toEqual([]);
	});
});

describe("CompositorHost keyboard", () => {
	function mount() {
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
			/>,
		);
		const root = view.container.querySelector(
			".oqto-compositor",
		) as HTMLElement;
		return { store, view, root };
	}

	it("moves focus directionally and toggles the sidebar", () => {
		const { store, root } = mount();
		fireEvent.keyDown(root, { key: "ArrowRight", altKey: true });
		expect(store.getSnapshot().focusedContentId).toBe(filesContent.id);
		fireEvent.keyDown(root, { key: "ArrowLeft", altKey: true });
		fireEvent.keyDown(root, { key: "ArrowLeft", altKey: true });
		expect(store.getSnapshot().focusedContentId).toBe(sessionsContent.id);
		fireEvent.keyDown(root, { key: "b", altKey: true });
		expect(containerByRole(store.getSnapshot(), "navigation").collapsed).toBe(
			true,
		);
		// Focus left the collapsed sidebar deterministically.
		expect(store.getSnapshot().focusedContentId).toBe(sessionsContent.id);
		fireEvent.keyDown(root, { key: "b", altKey: true });
		expect(containerByRole(store.getSnapshot(), "navigation").collapsed).toBe(
			false,
		);
	});

	it("moves the focused Content to a neighbor, cycles tabs, and undoes", () => {
		const { store, root } = mount();
		act(() => {
			store.commit([
				{ type: "open", content: gitContent, target: { role: "auxiliary" } },
				{ type: "focus", contentId: chatContent.id },
			]);
		});
		fireEvent.keyDown(root, {
			key: "ArrowRight",
			altKey: true,
			shiftKey: true,
		});
		expect(
			containerByRole(store.getSnapshot(), "auxiliary").stack.map((c) => c.id),
		).toEqual([filesContent.id, gitContent.id, chatContent.id]);
		expect(store.getSnapshot().focusedContentId).toBe(chatContent.id);
		fireEvent.keyDown(root, { key: "]", altKey: true });
		expect(store.getSnapshot().focusedContentId).toBe(filesContent.id);
		expect(
			containerByRole(store.getSnapshot(), "auxiliary").activeContentId,
		).toBe(filesContent.id);
		const revisionBefore = store.getSnapshot().revision;
		fireEvent.keyDown(root, { key: "z", altKey: true });
		expect(store.getSnapshot().revision).toBe(revisionBefore + 1);
		expect(store.getSnapshot().focusedContentId).toBe(chatContent.id);
		fireEvent.keyDown(root, { key: "z", altKey: true });
		expect(
			containerByRole(store.getSnapshot(), "primary").stack.map((c) => c.id),
		).toEqual([chatContent.id]);
	});

	it("resizes the focused column, closes focused Content, and ignores unbound keys", () => {
		const { store, root } = mount();
		const before = activeArrangementOf(store.getSnapshot()).grid.columns[1].size
			.value;
		fireEvent.keyDown(root, { key: "=", altKey: true });
		expect(
			activeArrangementOf(store.getSnapshot()).grid.columns[1].size.value,
		).toBeGreaterThan(before);
		const event = fireEvent.keyDown(root, { key: "x", altKey: true });
		expect(event).toBe(true);
		expect(store.getSnapshot().revision).toBe(1);
		fireEvent.keyDown(root, { key: "w", altKey: true });
		expect(containerByRole(store.getSnapshot(), "primary").stack).toHaveLength(
			0,
		);
	});
});
