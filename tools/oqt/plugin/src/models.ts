// Classic-pi model ergonomics for the oqt client: scoped models (the owner's
// `enabledModels`), a searchable /model picker with the scope first,
// Ctrl+P / Shift+Ctrl+P cycling and /scoped-models to edit the scope.
// Runs in the operator's client process, never in the confined brain.
import { copyFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Context } from "@earendil-works/chord";
import type {
	PresentationUI,
	SlashCommands,
} from "@earendil-works/pi-coding-agent/experimental/plugin";
import lockfile from "proper-lockfile";
import type {
	ModelSummary,
	Models,
} from "../../../../src/experimental/services/models.ts";

const LEVELS = ["off", "minimal", "low", "medium", "high", "xhigh"] as const;
type Level = (typeof LEVELS)[number];

interface ScopedEntry {
	model: ModelSummary;
	thinking?: Level;
}

function glob(pattern: string) {
	const escaped = pattern.replace(/[.+^${}()|\\]/g, "\\$&");
	return new RegExp(
		`^${escaped.replace(/\*/g, ".*").replace(/\?/g, ".")}$`,
		"i",
	);
}

/** pi's `enabledModels` patterns: `provider/id`, globs, optional `:thinking`. */
export function resolveScope(
	patterns: readonly string[],
	models: readonly ModelSummary[],
) {
	const scoped: ScopedEntry[] = [];
	const seen = new Set<string>();
	for (const raw of patterns) {
		let pattern = raw.trim();
		let thinking: Level | undefined;
		const colon = pattern.lastIndexOf(":");
		const suffix = colon >= 0 ? pattern.slice(colon + 1) : "";
		if ((LEVELS as readonly string[]).includes(suffix)) {
			thinking = suffix as Level;
			pattern = pattern.slice(0, colon);
		}
		const regex = glob(pattern);
		for (const model of models) {
			const id = `${model.provider}/${model.modelId}`;
			if (seen.has(id)) continue;
			if (regex.test(id) || regex.test(model.modelId)) {
				seen.add(id);
				scoped.push({ model, ...(thinking ? { thinking } : {}) });
			}
		}
	}
	return scoped;
}

function ownerSettings(owner: string) {
	try {
		const value = JSON.parse(
			readFileSync(join(owner, "settings.json"), "utf8"),
		);
		return Array.isArray(value.enabledModels)
			? (value.enabledModels as unknown[]).filter(
					(p): p is string => typeof p === "string",
				)
			: [];
	} catch {
		return [];
	}
}

/** Write only `enabledModels` the way pi itself does: under pi's own
 * settings lock, in place (stow/symlinked profiles stay intact), keeping every
 * other key, after a one-time backup of the original file. */
export function saveEnabledModels(
	owner: string,
	patterns: string[] | undefined,
) {
	const file = join(owner, "settings.json");
	const backup = join(owner, "settings.json.oqt-backup");
	if (!existsSync(file)) throw new Error("No owning pi settings.json");
	const release = lockfile.lockSync(file, { realpath: false });
	try {
		const text = readFileSync(file, "utf8");
		if (!existsSync(backup)) copyFileSync(file, backup);
		const value = JSON.parse(text) as Record<string, unknown>;
		if (patterns === undefined) delete value.enabledModels;
		else value.enabledModels = patterns;
		writeFileSync(file, JSON.stringify(value, null, 2), "utf8");
	} finally {
		release();
	}
}

export function installModelCommands(options: {
	commands: SlashCommands;
	ui: PresentationUI;
	models: Models;
	own: (dispose: () => void) => void;
}) {
	const { commands, ui, models } = options;
	const owner = process.env.OQT_OWNER_AGENT_DIR ?? "";
	const available = () => models.state.value?.catalog.availableModels ?? [];
	const current = () => models.state.value?.configuration.model ?? null;
	const scope = () =>
		resolveScope(owner ? ownerSettings(owner) : [], available());
	const key = (model: { provider: string; modelId: string }) =>
		`${model.provider}/${model.modelId}`;
	const apply = async (entry: ScopedEntry, context: Context) => {
		await models.select(
			{ provider: entry.model.provider, modelId: entry.model.modelId },
			context,
		);
		if (entry.thinking) await models.selectThinking(entry.thinking, context);
		ui.showStatus(
			`Model: ${key(entry.model)}${entry.thinking ? ` · thinking ${entry.thinking}` : ""}`,
			context,
		);
	};
	const cycle = async (direction: 1 | -1, context: Context) => {
		const scoped = scope();
		const list: ScopedEntry[] =
			scoped.length > 0 ? scoped : available().map((model) => ({ model }));
		if (list.length === 0) {
			ui.showStatus("No models available", context);
			return;
		}
		const active = current();
		const index = active
			? list.findIndex((entry) => key(entry.model) === key(active))
			: -1;
		const next = list[(index + direction + list.length) % list.length];
		if (next) await apply(next, context);
	};
	options.own(
		commands.replace({
			name: "model",
			description: "Select model (scoped models first; type to search)",
			argumentHint: "<provider/model>",
			getArgumentCompletions(prefix) {
				const needle = prefix.toLowerCase();
				return available()
					.filter((model) =>
						`${key(model)} ${model.name}`.toLowerCase().includes(needle),
					)
					.map((model) => ({
						value: key(model),
						label: model.modelId,
						description: model.provider,
					}));
			},
			async run(args, context) {
				const scoped = scope();
				const scopedKeys = new Set(scoped.map((entry) => key(entry.model)));
				if (args.trim()) {
					const model = available().find(
						(candidate) =>
							key(candidate) === args.trim() ||
							candidate.modelId === args.trim(),
					);
					if (!model) throw new Error(`Unknown model: ${args}`);
					await apply(
						scoped.find((entry) => key(entry.model) === key(model)) ?? {
							model,
						},
						context,
					);
					return undefined;
				}
				const active = current();
				const ordered = [
					...scoped,
					...available()
						.filter((model) => !scopedKeys.has(key(model)))
						.map((model) => ({ model }) as ScopedEntry),
				];
				const picked = await ui.select(
					scoped.length > 0
						? "Select model (★ = scoped, Ctrl+P cycles them)"
						: "Select model",
					ordered.map((entry) => ({
						value: key(entry.model),
						label: `${scopedKeys.has(key(entry.model)) ? "★ " : "  "}${entry.model.name}${active && key(active) === key(entry.model) ? "  (current)" : ""}`,
						description: `${key(entry.model)}${entry.thinking ? `:${entry.thinking}` : ""}`,
					})),
					active ? key(active) : undefined,
					context,
				);
				if (picked === undefined) return undefined;
				const entry = ordered.find(
					(candidate) => key(candidate.model) === picked,
				);
				if (entry) await apply(entry, context);
				return undefined;
			},
		}),
	);
	options.own(
		commands.replace({
			name: "model-next",
			description: "Next scoped model (Ctrl+P)",
			async run(_args, context) {
				await cycle(1, context);
				return undefined;
			},
		}),
	);
	options.own(
		commands.replace({
			name: "model-prev",
			description: "Previous scoped model (Shift+Ctrl+P)",
			async run(_args, context) {
				await cycle(-1, context);
				return undefined;
			},
		}),
	);
	options.own(
		commands.replace({
			name: "scoped-models",
			description:
				"Choose the models Ctrl+P cycles through (saved like pi's /scoped-models)",
			async run(_args, context) {
				if (!owner) throw new Error("No owning Pi profile");
				const selected = new Set(scope().map((entry) => key(entry.model)));
				const all = available();
				let focus: string | undefined;
				for (;;) {
					const picked = await ui.select(
						`Scoped models (${selected.size}) — Enter toggles, pick "Save" when done`,
						[
							{ value: "__save__", label: `✓ Save (${selected.size} models)` },
							{ value: "__clear__", label: "✗ Clear scope (cycle all models)" },
							...all.map((model) => ({
								value: key(model),
								label: `${selected.has(key(model)) ? "[x]" : "[ ]"} ${model.name}`,
								description: key(model),
							})),
						],
						focus,
						context,
					);
					if (picked === undefined) {
						ui.showStatus("Scoped models unchanged", context);
						return undefined;
					}
					if (picked === "__clear__") selected.clear();
					else if (picked !== "__save__") {
						if (selected.has(picked)) selected.delete(picked);
						else selected.add(picked);
						focus = picked;
						continue;
					}
					saveEnabledModels(
						owner,
						selected.size > 0 ? [...selected] : undefined,
					);
					ui.showStatus(
						selected.size > 0
							? `Saved ${selected.size} scoped models to your pi settings · Ctrl+P cycles`
							: "Cleared scoped models · Ctrl+P cycles all models",
						context,
					);
					return undefined;
				}
			},
		}),
	);
}
