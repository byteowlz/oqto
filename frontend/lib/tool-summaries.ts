/**
 * Generate a human-readable one-liner describing what a tool call is doing.
 * Pure pattern matching on tool name + arguments -- no LLM needed.
 */

type Locale = "en" | "de";

type Translations = {
	en: string;
	de: string;
};

function t(translations: Translations, locale: Locale): string {
	return translations[locale];
}

function truncPath(path: string, max = 50): string {
	if (path.length <= max) return path;
	// Keep filename, truncate directory
	const parts = path.split("/");
	const file = parts[parts.length - 1];
	if (file.length >= max - 4) return `...${file.slice(-(max - 3))}`;
	return `.../${parts.slice(-2).join("/")}`;
}

function truncStr(s: string, max = 60): string {
	if (s.length <= max) return s;
	return `${s.slice(0, max - 3)}...`;
}

// -- Bash command patterns --

type BashPattern = {
	match: (cmd: string) => boolean;
	summary: (cmd: string, locale: Locale) => string;
};

const bashPatterns: BashPattern[] = [
	// Package managers
	{
		match: (cmd) => /\b(npm|yarn|pnpm|bun)\s+(install|add|i)\b/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Installing packages", de: "Pakete installieren" }, locale),
	},
	{
		match: (cmd) => /\b(pip|uv)\s+(install|add)\b/.test(cmd),
		summary: (_cmd, locale) =>
			t(
				{
					en: "Installing Python packages",
					de: "Python-Pakete installieren",
				},
				locale,
			),
	},
	{
		match: (cmd) => /\bcargo\s+(install|add|build)\b/.test(cmd),
		summary: (cmd, locale) => {
			if (/\bbuild\b/.test(cmd))
				return t({ en: "Building Rust project", de: "Rust-Projekt bauen" }, locale);
			return t(
				{ en: "Installing Rust packages", de: "Rust-Pakete installieren" },
				locale,
			);
		},
	},
	// Git
	{
		match: (cmd) => /\bgit\s+clone\b/.test(cmd),
		summary: (_cmd, locale) =>
			t(
				{ en: "Cloning git repository", de: "Git-Repository klonen" },
				locale,
			),
	},
	{
		match: (cmd) => /\bgit\s+pull\b/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Pulling latest changes", de: "Neueste Aenderungen holen" }, locale),
	},
	{
		match: (cmd) => /\bgit\s+push\b/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Pushing changes", de: "Aenderungen pushen" }, locale),
	},
	{
		match: (cmd) => /\bgit\s+commit\b/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Committing changes", de: "Aenderungen committen" }, locale),
	},
	{
		match: (cmd) => /\bgit\s+(status|diff|log)\b/.test(cmd),
		summary: (cmd, locale) => {
			if (/\bstatus\b/.test(cmd))
				return t({ en: "Checking git status", de: "Git-Status pruefen" }, locale);
			if (/\bdiff\b/.test(cmd))
				return t({ en: "Showing changes", de: "Aenderungen anzeigen" }, locale);
			return t({ en: "Viewing git log", de: "Git-Log anzeigen" }, locale);
		},
	},
	// Search
	{
		match: (cmd) => /\b(grep|rg|ag)\s/.test(cmd),
		summary: (cmd, locale) => {
			const m = cmd.match(/(?:grep|rg|ag)\s+(?:-[^\s]*\s+)*['"]?([^'"|\s]+)/);
			const query = m ? m[1] : null;
			if (query) {
				return t(
					{
						en: `Searching for '${truncStr(query, 30)}'`,
						de: `Suche nach '${truncStr(query, 30)}'`,
					},
					locale,
				);
			}
			return t({ en: "Searching files", de: "Dateien durchsuchen" }, locale);
		},
	},
	// Find
	{
		match: (cmd) => /\bfind\s/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Finding files", de: "Dateien suchen" }, locale),
	},
	// Directory listing
	{
		match: (cmd) => /\b(ls|dir|tree)\s/.test(cmd) || /^ls\s*$/.test(cmd.trim()),
		summary: (_cmd, locale) =>
			t({ en: "Listing files", de: "Dateien auflisten" }, locale),
	},
	// File operations
	{
		match: (cmd) => /\b(cat|head|tail|less|more)\s/.test(cmd),
		summary: (cmd, locale) => {
			const m = cmd.match(/(?:cat|head|tail|less|more)\s+(.+?)(?:\s*[|;]|$)/);
			const file = m ? truncPath(m[1].trim()) : null;
			if (file)
				return t(
					{ en: `Reading ${file}`, de: `Lesen: ${file}` },
					locale,
				);
			return t({ en: "Reading file", de: "Datei lesen" }, locale);
		},
	},
	{
		match: (cmd) => /\bmkdir\b/.test(cmd),
		summary: (_cmd, locale) =>
			t(
				{ en: "Creating directory", de: "Verzeichnis erstellen" },
				locale,
			),
	},
	{
		match: (cmd) => /\b(rm|trash)\s/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Removing files", de: "Dateien entfernen" }, locale),
	},
	{
		match: (cmd) => /\b(cp|rsync)\s/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Copying files", de: "Dateien kopieren" }, locale),
	},
	{
		match: (cmd) => /\bmv\s/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Moving files", de: "Dateien verschieben" }, locale),
	},
	// Build/compile
	{
		match: (cmd) => /\b(make|cmake|ninja)\b/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Building project", de: "Projekt bauen" }, locale),
	},
	// Test
	{
		match: (cmd) =>
			/\b(test|jest|vitest|pytest|cargo\s+test|bun\s+test|npm\s+test)\b/.test(
				cmd,
			),
		summary: (_cmd, locale) =>
			t({ en: "Running tests", de: "Tests ausfuehren" }, locale),
	},
	// Docker
	{
		match: (cmd) => /\b(docker|podman)\s/.test(cmd),
		summary: (_cmd, locale) =>
			t(
				{ en: "Running container command", de: "Container-Befehl ausfuehren" },
				locale,
			),
	},
	// curl/wget
	{
		match: (cmd) => /\b(curl|wget)\s/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Fetching URL", de: "URL abrufen" }, locale),
	},
	// Python
	{
		match: (cmd) => /\b(python3?|uv\s+run)\s/.test(cmd),
		summary: (_cmd, locale) =>
			t(
				{ en: "Running Python script", de: "Python-Skript ausfuehren" },
				locale,
			),
	},
	// SSH/remote
	{
		match: (cmd) => /\bssh\s/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Running remote command", de: "Remote-Befehl ausfuehren" }, locale),
	},
	// systemctl
	{
		match: (cmd) => /\bsystemctl\s/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Managing service", de: "Dienst verwalten" }, locale),
	},
	// Web search tools
	{
		match: (cmd) => /\b(sx|exa-web-search|exa-code-context)\s/.test(cmd),
		summary: (cmd, locale) => {
			const m = cmd.match(/(?:sx|exa-web-search|exa-code-context)\s+['"]([^'"]+)/);
			const query = m ? m[1] : null;
			if (query)
				return t(
					{
						en: `Searching: ${truncStr(query, 40)}`,
						de: `Suche: ${truncStr(query, 40)}`,
					},
					locale,
				);
			return t({ en: "Web search", de: "Websuche" }, locale);
		},
	},
	// Scheduler
	{
		match: (cmd) => /\bskdlr\s/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Managing schedule", de: "Zeitplan verwalten" }, locale),
	},
	// tmpltr/sldr
	{
		match: (cmd) => /\btmpltr\s/.test(cmd),
		summary: (_cmd, locale) =>
			t({ en: "Generating document", de: "Dokument generieren" }, locale),
	},
	{
		match: (cmd) => /\bsldr\s/.test(cmd),
		summary: (_cmd, locale) =>
			t(
				{ en: "Building presentation", de: "Praesentation erstellen" },
				locale,
			),
	},
	// agntz memory
	{
		match: (cmd) => /\bagntz\s+memory\b/.test(cmd),
		summary: (cmd, locale) => {
			if (/\bsearch\b/.test(cmd))
				return t({ en: "Searching memories", de: "Erinnerungen durchsuchen" }, locale);
			if (/\badd\b/.test(cmd))
				return t({ en: "Saving to memory", de: "In Erinnerung speichern" }, locale);
			if (/\blist\b/.test(cmd))
				return t({ en: "Listing memories", de: "Erinnerungen auflisten" }, locale);
			return t({ en: "Memory operation", de: "Speicheroperation" }, locale);
		},
	},
];

function summarizeBash(
	command: string,
	locale: Locale,
): string | null {
	const cmd = command.trim();
	for (const pattern of bashPatterns) {
		if (pattern.match(cmd)) {
			return pattern.summary(cmd, locale);
		}
	}
	// Fallback: show truncated command
	return null;
}

// -- Main entry point --

export function getToolSummary(
	toolName: string,
	input: Record<string, unknown> | undefined,
	locale: Locale,
): string | null {
	const name = toolName.toLowerCase();

	if (name === "bash" || name === "execute_command") {
		const cmd =
			(input?.command as string) ?? (input?.cmd as string) ?? null;
		if (cmd) {
			const summary = summarizeBash(cmd, locale);
			if (summary) return summary;
			// Fallback: first meaningful part of command
			const clean = cmd
				.replace(/^(cd\s+[^\s;]+\s*[;&|]+\s*)+/, "") // strip leading cd
				.replace(/\s*2>&1.*$/, "") // strip redirects
				.replace(/\s*\|.*$/, "") // strip pipes (keep first command)
				.trim();
			return truncStr(clean, 60);
		}
	}

	if (name === "read") {
		const path = (input?.path as string) ?? null;
		if (path) {
			return t(
				{
					en: `Reading ${truncPath(path)}`,
					de: `Lesen: ${truncPath(path)}`,
				},
				locale,
			);
		}
	}

	if (name === "write") {
		const path = (input?.path as string) ?? null;
		if (path) {
			return t(
				{
					en: `Writing ${truncPath(path)}`,
					de: `Schreiben: ${truncPath(path)}`,
				},
				locale,
			);
		}
	}

	if (name === "edit") {
		const path = (input?.path as string) ?? null;
		if (path) {
			return t(
				{
					en: `Editing ${truncPath(path)}`,
					de: `Bearbeiten: ${truncPath(path)}`,
				},
				locale,
			);
		}
	}

	if (name === "glob") {
		const pattern = (input?.pattern as string) ?? null;
		if (pattern) {
			return t(
				{
					en: `Finding ${truncStr(pattern, 40)}`,
					de: `Suche: ${truncStr(pattern, 40)}`,
				},
				locale,
			);
		}
	}

	if (name === "grep") {
		const pattern = (input?.pattern as string) ?? (input?.query as string) ?? null;
		if (pattern) {
			return t(
				{
					en: `Searching for '${truncStr(pattern, 30)}'`,
					de: `Suche nach '${truncStr(pattern, 30)}'`,
				},
				locale,
			);
		}
	}

	if (name === "todowrite" || name === "todo_write") {
		return t(
			{ en: "Updating task list", de: "Aufgabenliste aktualisieren" },
			locale,
		);
	}

	if (name === "todoread" || name === "todo_read" || name === "todo") {
		const action = (input?.action as string) ?? null;
		if (action === "add")
			return t({ en: "Adding task", de: "Aufgabe hinzufuegen" }, locale);
		if (action === "update")
			return t({ en: "Updating task", de: "Aufgabe aktualisieren" }, locale);
		if (action === "remove")
			return t({ en: "Removing task", de: "Aufgabe entfernen" }, locale);
		return t(
			{ en: "Managing tasks", de: "Aufgaben verwalten" },
			locale,
		);
	}

	if (name === "self_reflection" || name === "selfReflection") {
		return t(
			{ en: "Checking context", de: "Kontext pruefen" },
			locale,
		);
	}

	if (name.includes("browser") || name.includes("screenshot")) {
		return t(
			{ en: "Browser interaction", de: "Browser-Interaktion" },
			locale,
		);
	}

	if (name.includes("search") || name.includes("web")) {
		return t({ en: "Web search", de: "Websuche" }, locale);
	}

	// No summary available
	return null;
}
