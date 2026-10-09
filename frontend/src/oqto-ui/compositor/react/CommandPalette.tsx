/**
 * Command palette: the keyboard-help and command-execution modal for
 * compositor actions. Rendered from the effective bindings data, so the
 * chords it shows are the ones that actually work. Pure React state — no
 * effects, no DOM queries; Escape closes, Enter runs the highlighted item.
 */

import { type KeyboardEvent, useState } from "react";
import type { PaletteLabels } from "./contracts";
import {
	type CompositorAction,
	type KeyBinding,
	actionId,
} from "./keybindings";

interface CommandPaletteProps {
	readonly bindings: readonly KeyBinding[];
	readonly labels: PaletteLabels;
	readonly onRun: (action: CompositorAction) => void;
	readonly onClose: () => void;
}

export function CommandPalette({
	bindings,
	labels,
	onRun,
	onClose,
}: CommandPaletteProps) {
	const [query, setQuery] = useState("");
	const [highlighted, setHighlighted] = useState(0);
	const entries = bindings
		.map((binding) => ({
			id: actionId(binding.action),
			action: binding.action,
			chord: binding.chord,
		}))
		.filter((entry) => entry.action.type !== "open-palette")
		.map((entry) => ({ ...entry, label: labels.actions[entry.id] ?? entry.id }))
		.filter((entry) =>
			entry.label.toLowerCase().includes(query.trim().toLowerCase()),
		);
	const active = Math.min(highlighted, Math.max(0, entries.length - 1));
	const run = (action: CompositorAction) => {
		onClose();
		onRun(action);
	};
	const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
		if (event.key === "Escape") {
			event.preventDefault();
			onClose();
		} else if (event.key === "ArrowDown") {
			event.preventDefault();
			setHighlighted(Math.min(active + 1, entries.length - 1));
		} else if (event.key === "ArrowUp") {
			event.preventDefault();
			setHighlighted(Math.max(active - 1, 0));
		} else if (event.key === "Enter" && entries[active]) {
			event.preventDefault();
			run(entries[active].action);
		}
	};
	return (
		<div
			className="oqto-compositor-palette-backdrop"
			onClick={onClose}
			onKeyDown={onKeyDown}
		>
			<div
				className="oqto-compositor-palette"
				role="dialog"
				aria-modal="true"
				aria-label={labels.title}
				onClick={(event) => event.stopPropagation()}
			>
				<input
					className="oqto-compositor-palette-search"
					type="search"
					// biome-ignore lint/a11y/noAutofocus: a modal palette must receive focus on open.
					autoFocus
					value={query}
					placeholder={labels.searchPlaceholder}
					aria-label={labels.searchPlaceholder}
					onChange={(event) => {
						setQuery(event.target.value);
						setHighlighted(0);
					}}
				/>
				<ul
					className="oqto-compositor-palette-list"
					role="listbox"
					aria-label={labels.title}
				>
					{entries.length === 0 ? (
						<li className="oqto-compositor-palette-empty">
							{labels.noMatches}
						</li>
					) : (
						entries.map((entry, index) => (
							<li
								key={entry.id}
								role="option"
								aria-selected={index === active}
								data-active={index === active || undefined}
								className="oqto-compositor-palette-item"
							>
								<button type="button" onClick={() => run(entry.action)}>
									<span>{entry.label}</span>
									<kbd>{entry.chord}</kbd>
								</button>
							</li>
						))
					)}
				</ul>
			</div>
		</div>
	);
}
