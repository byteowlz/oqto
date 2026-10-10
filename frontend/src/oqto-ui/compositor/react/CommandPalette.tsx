/**
 * Command palette: the keyboard-help and command-execution modal for
 * compositor actions. Rendered from the effective bindings data, so the
 * chords it shows are the ones that actually work. Pure React state — no
 * effects, no DOM queries; Escape closes, Enter runs the highlighted item.
 */

import { type KeyboardEvent, type MouseEvent, useId, useState } from "react";
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
	const listId = useId();
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
	// Only a click on the backdrop itself closes; clicks inside the dialog
	// bubble here too and must not dismiss it.
	const onBackdropClick = (event: MouseEvent<HTMLElement>) => {
		if (event.target === event.currentTarget) onClose();
	};
	const optionId = (index: number) => `${listId}-option-${index}`;
	return (
		<div
			className="oqto-compositor-palette-backdrop"
			onClick={onBackdropClick}
			onKeyDown={onKeyDown}
		>
			<dialog
				open
				className="oqto-compositor-palette"
				aria-modal="true"
				aria-label={labels.title}
			>
				{/* WAI-ARIA combobox: focus stays in the input, the highlighted
				    option is announced through aria-activedescendant. */}
				<input
					className="oqto-compositor-palette-search"
					type="search"
					role="combobox"
					aria-expanded="true"
					aria-controls={listId}
					aria-autocomplete="list"
					aria-activedescendant={entries[active] ? optionId(active) : undefined}
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
				{entries.length === 0 ? (
					<p className="oqto-compositor-palette-empty">{labels.noMatches}</p>
				) : null}
				<div
					id={listId}
					className="oqto-compositor-palette-list"
					// biome-ignore lint/a11y/useSemanticElements: a native <select> cannot render filtered commands with their key chords.
					role="listbox"
					tabIndex={-1}
					aria-label={labels.title}
				>
					{entries.map((entry, index) => (
						<div
							key={entry.id}
							id={optionId(index)}
							// biome-ignore lint/a11y/useSemanticElements: options of the combobox listbox above, not a native <select>.
							role="option"
							tabIndex={-1}
							aria-selected={index === active}
							data-active={index === active || undefined}
							className="oqto-compositor-palette-item"
							onClick={() => run(entry.action)}
							onKeyDown={(event) => {
								if (event.key === "Enter") run(entry.action);
							}}
						>
							<span>{entry.label}</span>
							<kbd>{entry.chord}</kbd>
						</div>
					))}
				</div>
			</dialog>
		</div>
	);
}
