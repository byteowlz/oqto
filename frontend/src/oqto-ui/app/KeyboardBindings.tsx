/**
 * Rebinding keys, one action per row. The list is the action registry, so
 * an action with no chord shows as unbound rather than disappearing.
 * Recording captures the next chord pressed on the row itself; saving keeps
 * the device-local layer the shell merges over the resolved config.
 */

import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
	DEFAULT_KEY_BINDINGS,
	type KeyBinding,
	actionId,
} from "../compositor/react/keybindings";
import type { BindingOverride } from "../platform/binding-storage";
import { captureChord } from "./chords";
import { ACTION_LABEL_KEYS } from "./compositorLabels";

interface KeyboardBindingsProps {
	/** The effective bindings after every layer has been merged. */
	readonly bindings: readonly KeyBinding[];
	readonly overrides: readonly BindingOverride[];
	readonly onChange: (overrides: readonly BindingOverride[]) => void;
}

export function KeyboardBindings({
	bindings,
	overrides,
	onChange,
}: KeyboardBindingsProps) {
	const { t } = useTranslation();
	const [recording, setRecording] = useState<string | null>(null);
	const chordFor = (id: string) =>
		bindings.find((binding) => actionId(binding.action) === id)?.chord ?? null;
	const rebind = (id: string, keys: string) => {
		setRecording(null);
		// One chord holds one action: a chord taken from another row is released.
		onChange([
			...overrides.filter(
				(override) => override.action !== id && override.keys !== keys,
			),
			{ keys, action: id },
		]);
	};
	return (
		<section className="wb-settings-pane__section">
			<h2>{t("oqtoUi.keyboard.label")}</h2>
			<ul className="wb-keys">
				{DEFAULT_KEY_BINDINGS.map((binding) => {
					const id = actionId(binding.action);
					const overridden = overrides.some(
						(override) => override.action === id,
					);
					const chord = chordFor(id);
					return (
						<li key={id}>
							<span>
								{t(`oqtoUi.compositor.actions.${ACTION_LABEL_KEYS[id] ?? id}`)}
							</span>
							<button
								type="button"
								data-recording={recording === id || undefined}
								data-overridden={overridden || undefined}
								aria-label={t("oqtoUi.keyboard.rebind")}
								onClick={() => setRecording(recording === id ? null : id)}
								onKeyDown={(event) => {
									if (recording !== id) return;
									event.preventDefault();
									if (event.key === "Escape") {
										setRecording(null);
										return;
									}
									const keys = captureChord(event);
									if (keys) rebind(id, keys);
								}}
							>
								{recording === id
									? t("oqtoUi.keyboard.press")
									: (chord ?? t("oqtoUi.keyboard.unbound"))}
							</button>
							{overridden ? (
								<button
									type="button"
									className="wb-keys__reset"
									aria-label={t("oqtoUi.keyboard.reset")}
									onClick={() =>
										onChange(
											overrides.filter((override) => override.action !== id),
										)
									}
								>
									{"↺"}
								</button>
							) : null}
						</li>
					);
				})}
			</ul>
		</section>
	);
}
