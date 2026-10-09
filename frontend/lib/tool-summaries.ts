/**
 * Generate a human-readable one-liner describing what a tool call is doing.
 * Pure pattern matching on tool name + arguments -- no LLM needed.
 *
 * Descriptions are anchored to parsed command words (see `@/lib/shell-command`).
 * A label is only produced when the command is actually recognized; anything
 * else falls back to the literal command, because showing `df -h /` is honest
 * while guessing "Removing files" from an unrelated word is not.
 *
 * Uses i18next for translations instead of inline locale ternaries.
 */

import { i18n } from "@/lib/i18n";
import {
	type ShellInvocation,
	firstOperand,
	parseShellCommand,
} from "@/lib/shell-command";

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

/**
 * Describe one invocation, or return null when it is not confidently known.
 * Returning null is meaningful: the caller then shows the real command.
 */
type Describer = (invocation: ShellInvocation) => string | null;

/** Map a subcommand onto a translation key, e.g. `git commit`. */
function bySubcommand(map: Record<string, string>): Describer {
	return (invocation) => {
		const sub = firstOperand(invocation.args);
		const key = sub ? map[sub] : undefined;
		return key ? i18n.t(key) : null;
	};
}

/** Always describe the command the same way, regardless of its arguments. */
function always(key: string): Describer {
	return () => i18n.t(key);
}

/** Search flags that consume their argument without being the pattern. */
const SEARCH_VALUE_FLAGS = new Set([
	"-m",
	"-A",
	"-B",
	"-C",
	"-f",
	"-g",
	"-t",
	"-T",
	"-d",
	"-j",
	"--max-count",
	"--include",
	"--exclude",
	"--glob",
	"--type",
]);

/** `-e PATTERN` names the pattern explicitly and outranks positional guessing. */
const SEARCH_PATTERN_FLAGS = new Set(["-e", "--regexp", "--pattern"]);

const describeSearch: Describer = (invocation) => {
	const explicit = invocation.args.findIndex((arg) =>
		SEARCH_PATTERN_FLAGS.has(arg),
	);
	const query =
		explicit === -1
			? firstOperand(invocation.args, SEARCH_VALUE_FLAGS)
			: (invocation.args[explicit + 1] ?? null);
	if (!query) return i18n.t("tools.searchingFiles");
	return i18n.t("tools.searchingFor", { query: truncStr(query, 30) });
};

/** `head`/`tail` take counts as flag values; `cat` has no value flags. */
const LINE_COUNT_FLAGS = new Set(["-n", "-c", "--lines", "--bytes"]);

function readDescriber(valueFlags?: ReadonlySet<string>): Describer {
	return (invocation) => {
		const path = firstOperand(invocation.args, valueFlags);
		if (!path) return i18n.t("tools.readingFile");
		return i18n.t("tools.readingPath", { path: truncPath(path) });
	};
}

const describeRead = readDescriber();

const describeWebSearch: Describer = (invocation) => {
	const query = firstOperand(invocation.args);
	if (!query) return i18n.t("tools.webSearch");
	return i18n.t("tools.searchingQuery", { query: truncStr(query, 40) });
};

const describeBrowser: Describer = (invocation) => {
	const map: Record<string, string> = {
		open: "tools.browserOpen",
		snapshot: "tools.browserSnapshot",
		click: "tools.browserClick",
		dblclick: "tools.browserClick",
		fill: "tools.browserFill",
		type: "tools.browserFill",
		press: "tools.browserPress",
		screenshot: "tools.browserScreenshot",
		console: "tools.browserConsole",
		eval: "tools.browserEval",
		wait: "tools.browserWait",
		scroll: "tools.browserScroll",
		close: "tools.browserClose",
	};
	const sub = firstOperand(invocation.args);
	const key = sub ? map[sub] : undefined;
	return key ? i18n.t(key) : i18n.t("tools.browserInteraction");
};

/** Package managers share verbs but not defaults, so each maps its own. */
const describeNodePackageManager: Describer = (invocation) => {
	const sub = firstOperand(invocation.args);
	if (!sub) return null;
	if (sub === "install" || sub === "add" || sub === "i" || sub === "ci") {
		return i18n.t("tools.installingPackages");
	}
	if (sub === "test") return i18n.t("tools.runningTests");
	if (sub === "run" || sub === "exec" || sub === "x") {
		const script = invocation.args.filter((arg) => !arg.startsWith("-"))[1];
		if (script === "build") return i18n.t("tools.buildingProject");
		if (script === "test") return i18n.t("tools.runningTests");
		if (script?.includes("lint")) return i18n.t("tools.lintingCode");
		if (script?.includes("format") || script?.includes("fmt")) {
			return i18n.t("tools.formattingCode");
		}
		if (script?.includes("typecheck") || script?.includes("tsc")) {
			return i18n.t("tools.checkingTypes");
		}
		return null;
	}
	return null;
};

const describeCargo: Describer = bySubcommand({
	install: "tools.installingRustPackages",
	add: "tools.installingRustPackages",
	build: "tools.buildingRustProject",
	b: "tools.buildingRustProject",
	check: "tools.checkingRustProject",
	c: "tools.checkingRustProject",
	test: "tools.runningTests",
	t: "tools.runningTests",
	clippy: "tools.lintingRustProject",
	fmt: "tools.formattingRustCode",
	run: "tools.runningRustProject",
	r: "tools.runningRustProject",
});

const describeGit: Describer = bySubcommand({
	clone: "tools.cloningGitRepo",
	pull: "tools.pullingLatestChanges",
	fetch: "tools.pullingLatestChanges",
	push: "tools.pushingChanges",
	commit: "tools.committingChanges",
	status: "tools.checkingGitStatus",
	diff: "tools.showingChanges",
	show: "tools.showingChanges",
	log: "tools.viewingGitLog",
	blame: "tools.viewingGitLog",
	add: "tools.stagingChanges",
	restore: "tools.stagingChanges",
	checkout: "tools.switchingBranch",
	switch: "tools.switchingBranch",
	branch: "tools.switchingBranch",
});

const describeMemory: Describer = (invocation) => {
	const operands = invocation.args.filter((arg) => !arg.startsWith("-"));
	if (operands[0] !== "memory") return null;
	const action = operands[1];
	if (action === "search") return i18n.t("tools.searchingMemories");
	if (action === "add") return i18n.t("tools.savingToMemory");
	if (action === "list") return i18n.t("tools.listingMemories");
	return i18n.t("tools.memoryOperation");
};

/**
 * Descriptions keyed by the parsed command word.
 *
 * Only add an entry when the command's intent is unambiguous from its name:
 * an honest `wc -l` beats an invented phrase.
 */
const DESCRIBERS: Record<string, Describer> = {
	// Search
	grep: describeSearch,
	rg: describeSearch,
	ag: describeSearch,
	ack: describeSearch,
	// Files
	ls: always("tools.listingFiles"),
	dir: always("tools.listingFiles"),
	tree: always("tools.listingFiles"),
	exa: always("tools.listingFiles"),
	eza: always("tools.listingFiles"),
	lsd: always("tools.listingFiles"),
	find: always("tools.findingFiles"),
	fd: always("tools.findingFiles"),
	fdfind: always("tools.findingFiles"),
	cat: describeRead,
	bat: describeRead,
	head: readDescriber(LINE_COUNT_FLAGS),
	tail: readDescriber(LINE_COUNT_FLAGS),
	less: describeRead,
	more: describeRead,
	mkdir: always("tools.creatingDirectory"),
	touch: always("tools.creatingFile"),
	rm: always("tools.removingFiles"),
	rmdir: always("tools.removingFiles"),
	trash: always("tools.removingFiles"),
	unlink: always("tools.removingFiles"),
	cp: always("tools.copyingFiles"),
	rsync: always("tools.copyingFiles"),
	scp: always("tools.copyingFiles"),
	mv: always("tools.movingFiles"),
	ln: always("tools.linkingFiles"),
	chmod: always("tools.changingPermissions"),
	chown: always("tools.changingPermissions"),
	chgrp: always("tools.changingPermissions"),
	tar: always("tools.archivingFiles"),
	zip: always("tools.archivingFiles"),
	unzip: always("tools.archivingFiles"),
	gzip: always("tools.archivingFiles"),
	diff: always("tools.comparingFiles"),
	delta: always("tools.comparingFiles"),
	// System
	df: always("tools.checkingDiskUsage"),
	du: always("tools.checkingDiskUsage"),
	free: always("tools.checkingDiskUsage"),
	ps: always("tools.inspectingProcesses"),
	top: always("tools.inspectingProcesses"),
	htop: always("tools.inspectingProcesses"),
	pgrep: always("tools.inspectingProcesses"),
	kill: always("tools.stoppingProcess"),
	pkill: always("tools.stoppingProcess"),
	killall: always("tools.stoppingProcess"),
	systemctl: always("tools.managingService"),
	launchctl: always("tools.managingService"),
	service: always("tools.managingService"),
	journalctl: always("tools.readingServiceLogs"),
	which: always("tools.locatingExecutable"),
	whereis: always("tools.locatingExecutable"),
	ssh: always("tools.runningRemoteCommand"),
	tmux: always("tools.managingTerminalSession"),
	// Data
	jq: always("tools.processingJson"),
	yq: always("tools.processingJson"),
	sqlite3: always("tools.queryingDatabase"),
	duckdb: always("tools.queryingDatabase"),
	psql: always("tools.queryingDatabase"),
	mysql: always("tools.queryingDatabase"),
	// Languages and builds
	git: describeGit,
	cargo: describeCargo,
	npm: describeNodePackageManager,
	pnpm: describeNodePackageManager,
	yarn: describeNodePackageManager,
	bun: describeNodePackageManager,
	node: always("tools.runningNodeScript"),
	deno: always("tools.runningNodeScript"),
	tsc: always("tools.checkingTypes"),
	biome: always("tools.lintingCode"),
	eslint: always("tools.lintingCode"),
	prettier: always("tools.formattingCode"),
	python: always("tools.runningPythonScript"),
	python3: always("tools.runningPythonScript"),
	pytest: always("tools.runningTests"),
	vitest: always("tools.runningTests"),
	jest: always("tools.runningTests"),
	pip: bySubcommand({
		install: "tools.installingPythonPackages",
		uninstall: "tools.installingPythonPackages",
	}),
	uv: bySubcommand({
		add: "tools.installingPythonPackages",
		sync: "tools.installingPythonPackages",
		pip: "tools.installingPythonPackages",
		run: "tools.runningPythonScript",
	}),
	make: always("tools.buildingProject"),
	cmake: always("tools.buildingProject"),
	ninja: always("tools.buildingProject"),
	docker: always("tools.runningContainerCommand"),
	podman: always("tools.runningContainerCommand"),
	kubectl: always("tools.runningContainerCommand"),
	curl: always("tools.fetchingUrl"),
	wget: always("tools.fetchingUrl"),
	// Workspace tooling
	"agent-browser": describeBrowser,
	"oqto-browser": describeBrowser,
	sx: describeWebSearch,
	"exa-web-search": describeWebSearch,
	agntz: describeMemory,
	mmry: always("tools.memoryOperation"),
	trx: always("tools.managingIssues"),
	skdlr: always("tools.managingSchedule"),
	tmpltr: always("tools.generatingDocument"),
	sldr: always("tools.buildingPresentation"),
	just: always("tools.runningTask"),
};

/**
 * The literal command, kept short. Used whenever the command is not known:
 * a reader can always act on `df -h /`, but never on a wrong guess.
 */
function invocationSignature(invocation: ShellInvocation): string {
	const words = [invocation.name, ...invocation.args.slice(0, 2)];
	return truncStr(words.join(" ").trim(), 44);
}

function summarizeBash(command: string): string | null {
	const { primary } = parseShellCommand(command);
	if (!primary) return null;
	const described = DESCRIBERS[primary.name]?.(primary) ?? null;
	const label = described ?? invocationSignature(primary);
	if (!label) return null;
	// The host matters more than the verb when work happens on another machine.
	return primary.remoteHost ? `${primary.remoteHost}: ${label}` : label;
}

// -- Main entry point --
// The locale parameter is kept for API compatibility but no longer used
// internally. i18n.t() reads the current language from the i18n instance.

export function getToolSummary(
	toolName: string,
	input: Record<string, unknown> | undefined,
	_locale?: string,
): string | null {
	const name = toolName.toLowerCase();

	if (name === "bash" || name === "execute_command") {
		const cmd = (input?.command as string) ?? (input?.cmd as string) ?? null;
		if (cmd) return summarizeBash(cmd);
		// Input not yet available (streaming) — return generic label
		return i18n.t("tools.runningCommand", {
			defaultValue: "Running command",
		});
	}

	if (name === "read") {
		const path =
			(input?.path as string) ??
			(input?.file_path as string) ??
			(input?.filePath as string) ??
			null;
		if (path) return i18n.t("tools.readingPath", { path: truncPath(path) });
		return i18n.t("tools.readingFile");
	}

	if (name === "write") {
		const path =
			(input?.path as string) ??
			(input?.file_path as string) ??
			(input?.filePath as string) ??
			null;
		if (path) return i18n.t("tools.writingPath", { path: truncPath(path) });
		return i18n.t("tools.writingFile", { defaultValue: "Writing file" });
	}

	if (name === "edit") {
		const path =
			(input?.path as string) ??
			(input?.file_path as string) ??
			(input?.filePath as string) ??
			null;
		if (path) return i18n.t("tools.editingPath", { path: truncPath(path) });
		return i18n.t("tools.editingFile", { defaultValue: "Editing file" });
	}

	if (name === "glob") {
		const pattern = (input?.pattern as string) ?? null;
		if (pattern)
			return i18n.t("tools.findingPattern", { pattern: truncStr(pattern, 40) });
	}

	if (name === "grep") {
		const pattern =
			(input?.pattern as string) ?? (input?.query as string) ?? null;
		if (pattern)
			return i18n.t("tools.searchingFor", { query: truncStr(pattern, 30) });
	}

	if (name === "todowrite" || name === "todo_write") {
		return i18n.t("tools.updatingTaskList");
	}

	if (name === "todoread" || name === "todo_read" || name === "todo") {
		const action = (input?.action as string) ?? null;
		if (action === "add") return i18n.t("tools.addingTask");
		if (action === "update") return i18n.t("tools.updatingTask");
		if (action === "remove") return i18n.t("tools.removingTask");
		return i18n.t("tools.managingTasks");
	}

	if (name === "self_reflection" || name === "selfReflection") {
		return i18n.t("tools.checkingContext");
	}

	if (name.includes("browser") || name.includes("screenshot")) {
		return i18n.t("tools.browserInteraction");
	}

	if (name.includes("search") || name.includes("web")) {
		return i18n.t("tools.webSearch");
	}

	// No summary available
	return null;
}
