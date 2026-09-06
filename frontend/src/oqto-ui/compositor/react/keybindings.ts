/**
 * Keyboard bindings as data (ADR-0040 Binding -> Action shape) resolved to
 * semantic compositor commands. Pure: no DOM, no store; the adapter matches
 * a key event against the binding list and commits the resolved commands.
 * Actions have stable string ids (`compositor.*`) so configuration layers
 * and the command palette can address them.
 */

import type {
	Container,
	LayoutCommand,
	LayoutSnapshot,
	SolvedLayout,
	SplitEdge,
} from "../index";
import { neighborContainer } from "../navigation";
import { resizeCommands, scrollByColumns } from "./gestures";

export type CompositorAction =
	| { readonly type: "toggle-navigation" }
	| { readonly type: "focus-neighbor"; readonly direction: SplitEdge }
	| { readonly type: "move-to-neighbor"; readonly direction: SplitEdge }
	| { readonly type: "resize-focused"; readonly deltaPx: number }
	| { readonly type: "cycle-tab"; readonly delta: 1 | -1 }
	| { readonly type: "close-focused" }
	| { readonly type: "scroll"; readonly delta: 1 | -1 }
	| { readonly type: "undo" }
	| { readonly type: "open-palette" };

/** Chord syntax: modifiers joined by "+", then the DOM `key` value. */
export interface KeyBinding {
	readonly chord: string;
	readonly action: CompositorAction;
}

export interface KeyChord {
	readonly key: string;
	readonly altKey: boolean;
	readonly ctrlKey: boolean;
	readonly metaKey: boolean;
	readonly shiftKey: boolean;
}

const DIRECTION_IDS: Record<SplitEdge, string> = {
	"inline-start": "start",
	"inline-end": "end",
	"block-start": "up",
	"block-end": "down",
};

/** Stable configuration/palette id for an action. */
export function actionId(action: CompositorAction): string {
	switch (action.type) {
		case "toggle-navigation":
			return "compositor.toggleNavigation";
		case "focus-neighbor":
			return `compositor.focus.${DIRECTION_IDS[action.direction]}`;
		case "move-to-neighbor":
			return `compositor.move.${DIRECTION_IDS[action.direction]}`;
		case "resize-focused":
			return action.deltaPx < 0 ? "compositor.shrink" : "compositor.grow";
		case "cycle-tab":
			return action.delta < 0 ? "compositor.prevTab" : "compositor.nextTab";
		case "close-focused":
			return "compositor.closeFocused";
		case "scroll":
			return action.delta < 0
				? "compositor.scrollStart"
				: "compositor.scrollEnd";
		case "undo":
			return "compositor.undo";
		case "open-palette":
			return "shell.openCommandPalette";
	}
}

/** Alt-based defaults: Super collides with the OS, Ctrl with the browser. */
export const DEFAULT_KEY_BINDINGS: readonly KeyBinding[] = [
	{ chord: "Alt+b", action: { type: "toggle-navigation" } },
	{
		chord: "Alt+ArrowLeft",
		action: { type: "focus-neighbor", direction: "inline-start" },
	},
	{
		chord: "Alt+ArrowRight",
		action: { type: "focus-neighbor", direction: "inline-end" },
	},
	{
		chord: "Alt+ArrowUp",
		action: { type: "focus-neighbor", direction: "block-start" },
	},
	{
		chord: "Alt+ArrowDown",
		action: { type: "focus-neighbor", direction: "block-end" },
	},
	{
		chord: "Alt+Shift+ArrowLeft",
		action: { type: "move-to-neighbor", direction: "inline-start" },
	},
	{
		chord: "Alt+Shift+ArrowRight",
		action: { type: "move-to-neighbor", direction: "inline-end" },
	},
	{
		chord: "Alt+Shift+ArrowUp",
		action: { type: "move-to-neighbor", direction: "block-start" },
	},
	{
		chord: "Alt+Shift+ArrowDown",
		action: { type: "move-to-neighbor", direction: "block-end" },
	},
	{ chord: "Alt+-", action: { type: "resize-focused", deltaPx: -40 } },
	{ chord: "Alt+=", action: { type: "resize-focused", deltaPx: 40 } },
	{ chord: "Alt+[", action: { type: "cycle-tab", delta: -1 } },
	{ chord: "Alt+]", action: { type: "cycle-tab", delta: 1 } },
	{ chord: "Alt+w", action: { type: "close-focused" } },
	{ chord: "Alt+z", action: { type: "undo" } },
	{ chord: "Alt+Shift+,", action: { type: "scroll", delta: -1 } },
	{ chord: "Alt+Shift+.", action: { type: "scroll", delta: 1 } },
	{ chord: "Ctrl+Shift+p", action: { type: "open-palette" } },
];

/** Resolves a stable id back to its action; null for unknown ids (fail closed). */
export function actionFromId(id: string): CompositorAction | null {
	return (
		DEFAULT_KEY_BINDINGS.find((binding) => actionId(binding.action) === id)
			?.action ?? null
	);
}

function chordOf(event: KeyChord): string {
	const parts = [];
	if (event.ctrlKey) parts.push("Ctrl");
	if (event.altKey) parts.push("Alt");
	if (event.metaKey) parts.push("Meta");
	if (event.shiftKey) parts.push("Shift");
	const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
	// Shifted punctuation reports the shifted glyph; bindings name the base key.
	const normalized = event.shiftKey
		? ({ "<": ",", ">": ".", "{": "[", "}": "]", _: "-", "+": "=" }[key] ?? key)
		: key;
	parts.push(normalized);
	return parts.join("+");
}

export function matchBinding(
	event: KeyChord,
	bindings: readonly KeyBinding[],
): CompositorAction | null {
	const chord = chordOf(event);
	return bindings.find((binding) => binding.chord === chord)?.action ?? null;
}

function focusedContainer(snapshot: LayoutSnapshot): Container | null {
	const focus = snapshot.focusedContentId;
	if (focus === null) return null;
	return (
		snapshot.containers.find((container) =>
			container.stack.some((content) => content.id === focus),
		) ?? null
	);
}

/**
 * Resolves an action to commands against the canonical snapshot and the
 * currently solved geometry. Returns [] when the action does not apply
 * (nothing focused, no neighbor, at a bound); `undo` and `open-palette`
 * are host-level and yield no commands here.
 */
export function resolveAction(
	snapshot: LayoutSnapshot,
	geometry: SolvedLayout,
	action: CompositorAction,
): LayoutCommand[] {
	const arrangement = snapshot.arrangements.find(
		(candidate) => candidate.id === snapshot.activeArrangementId,
	);
	if (!arrangement) return [];
	const focused = focusedContainer(snapshot);
	switch (action.type) {
		case "toggle-navigation": {
			const navigation = snapshot.containers.find(
				(container) =>
					container.role === "navigation" &&
					arrangement.grid.placements.some(
						(p) => p.containerId === container.id,
					),
			);
			return navigation
				? [
						{
							type: "collapse",
							containerId: navigation.id,
							collapsed: !navigation.collapsed,
						},
					]
				: [];
		}
		case "focus-neighbor": {
			if (!focused) return [];
			const neighborId = neighborContainer(
				arrangement,
				snapshot.containers,
				focused.id,
				action.direction,
			);
			const neighbor = neighborId
				? snapshot.containers.find((container) => container.id === neighborId)
				: null;
			const target = neighbor?.activeContentId ?? neighbor?.stack[0]?.id;
			return target ? [{ type: "focus", contentId: target }] : [];
		}
		case "move-to-neighbor": {
			if (!focused || snapshot.focusedContentId === null) return [];
			const neighborId = neighborContainer(
				arrangement,
				snapshot.containers,
				focused.id,
				action.direction,
			);
			return neighborId
				? [
						{
							type: "move",
							contentId: snapshot.focusedContentId,
							destination: { containerId: neighborId },
						},
					]
				: [
						{
							type: "split",
							contentId: snapshot.focusedContentId,
							edge: action.direction,
						},
					];
		}
		case "resize-focused": {
			if (!focused) return [];
			const placement = arrangement.grid.placements.find(
				(p) => p.containerId === focused.id,
			);
			if (!placement) return [];
			const last =
				placement.column + placement.colSpan >= arrangement.grid.columns.length;
			return last
				? resizeCommands(
						arrangement,
						geometry,
						"inline",
						placement.column - 1,
						-action.deltaPx,
					)
				: resizeCommands(
						arrangement,
						geometry,
						"inline",
						placement.column + placement.colSpan - 1,
						action.deltaPx,
					);
		}
		case "cycle-tab": {
			if (!focused || focused.stack.length < 2) return [];
			const index = focused.stack.findIndex(
				(content) => content.id === focused.activeContentId,
			);
			const next =
				focused.stack[
					(index + action.delta + focused.stack.length) % focused.stack.length
				];
			return [
				{ type: "activate", contentId: next.id },
				{ type: "focus", contentId: next.id },
			];
		}
		case "close-focused":
			return snapshot.focusedContentId === null
				? []
				: [{ type: "close", contentId: snapshot.focusedContentId }];
		case "scroll": {
			const command = scrollByColumns(arrangement, action.delta);
			return command ? [command] : [];
		}
		case "undo":
		case "open-palette":
			return [];
	}
}
