// Slash command system for chat input
// Typing "/" shows a popup list of available commands with fuzzy filtering

import type { CommandInfo } from "./agent-client";

export interface SlashCommand {
	name: string; // Command name (without slash)
	description: string; // Short description shown in popup
	icon?: string; // Optional lucide icon name
}

// Icon mapping for known commands
const knownIcons: Record<string, string> = {
	init: "Sparkles",
	review: "Eye",
	inbox: "Inbox",
	mail: "Mail",
	browser: "Globe",
	"close-browser": "X",
};

// Built-in UI commands shown alongside agent commands.
export const builtInCommands: SlashCommand[] = [
	{ name: "browser", description: "Show the browser stream" },
	{ name: "close-browser", description: "Hide the browser stream" },
];

// Convert agent command list to SlashCommand format
// Only shows commands that are available via the /command API
export function commandInfoToSlashCommands(
	commands: CommandInfo[],
): SlashCommand[] {
	return commands.map((cmd) => ({
		name: cmd.name,
		description:
			cmd.description ||
			cmd.template.slice(0, 60) + (cmd.template.length > 60 ? "..." : ""),
		icon: knownIcons[cmd.name] || "Terminal",
	}));
}

// Simple fuzzy match - matches if all characters appear in order
export function fuzzyMatch(query: string, text: string): boolean {
	const q = query.toLowerCase();
	const t = text.toLowerCase();
	let qi = 0;
	for (let ti = 0; ti < t.length && qi < q.length; ti++) {
		if (t[ti] === q[qi]) qi++;
	}
	return qi === q.length;
}

// Filter and score commands by query
export function filterCommands(
	commands: SlashCommand[],
	query: string,
): SlashCommand[] {
	if (!query) return commands;

	return commands
		.filter(
			(cmd) =>
				fuzzyMatch(query, cmd.name) || fuzzyMatch(query, cmd.description),
		)
		.sort((a, b) => {
			// Prioritize exact name matches
			const aExact = a.name.toLowerCase() === query.toLowerCase();
			const bExact = b.name.toLowerCase() === query.toLowerCase();
			if (aExact && !bExact) return -1;
			if (!aExact && bExact) return 1;

			// Then prefix matches
			const aNameMatch = a.name.toLowerCase().startsWith(query.toLowerCase());
			const bNameMatch = b.name.toLowerCase().startsWith(query.toLowerCase());
			if (aNameMatch && !bNameMatch) return -1;
			if (!aNameMatch && bNameMatch) return 1;

			return a.name.localeCompare(b.name);
		});
}

// Parse slash command from input text
// Returns { isSlash, command, args } if input starts with /
// e.g., "/fix bug" -> { isSlash: true, command: "fix", args: "bug" }
export function parseSlashInput(text: string): {
	isSlash: boolean;
	command: string;
	args: string;
} {
	if (!text.startsWith("/")) {
		return { isSlash: false, command: "", args: "" };
	}

	const withoutSlash = text.slice(1);
	const spaceIndex = withoutSlash.indexOf(" ");

	if (spaceIndex === -1) {
		// Still typing command name
		return { isSlash: true, command: withoutSlash, args: "" };
	}

	// Command completed, rest is args
	return {
		isSlash: true,
		command: withoutSlash.slice(0, spaceIndex),
		args: withoutSlash.slice(spaceIndex + 1),
	};
}
