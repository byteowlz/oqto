/**
 * Deterministic shell parsing for tool-call labels.
 *
 * Labels used to be derived from substring matches against the whole command,
 * so `df -h /var/lib/docker` was reported as a container command and any
 * pipeline containing a later `rm` was reported as deleting files. Anchoring a
 * label to a parsed command word is the only way to keep it truthful.
 *
 * This is a display-only parser: it recovers command words, not shell
 * semantics, and must never be used for authorization or safety decisions.
 */

/** A single command invocation, with wrappers such as `sudo` unwrapped. */
export type ShellInvocation = {
	/** Command word without its directory prefix, e.g. `git`. */
	name: string;
	/** Arguments after the command word, with one level of quoting removed. */
	args: string[];
	/** Host this invocation runs on when it was reached through `ssh`. */
	remoteHost?: string;
};

export type ParsedShellCommand = {
	/** Invocations in execution order. */
	invocations: ShellInvocation[];
	/** The invocation that best represents what the command is doing. */
	primary: ShellInvocation | null;
};

/** Long inputs are heredoc payloads or generated scripts; labels never need them. */
const MAX_SOURCE_LENGTH = 20_000;
const MAX_DEPTH = 3;

/** Commands that set up or narrate a command line rather than perform its work. */
const INCIDENTAL_COMMANDS = new Set([
	"cd",
	"pushd",
	"popd",
	"export",
	"set",
	"unset",
	"source",
	".",
	":",
	"true",
	"false",
	"echo",
	"printf",
	"sleep",
	"wait",
	"clear",
	"umask",
	"shopt",
	"alias",
	"pwd",
]);

/** Keywords that introduce a compound statement with no command of their own. */
const STATEMENT_KEYWORDS = new Set([
	"for",
	"while",
	"until",
	"if",
	"case",
	"esac",
	"done",
	"fi",
	"in",
	"select",
	"function",
]);

/** Keywords that merely prefix the command that follows them. */
const PREFIX_KEYWORDS = new Set(["do", "then", "else", "elif", "!"]);

/** Wrappers that execute another command, with the flags that take a value. */
const WRAPPERS: Record<
	string,
	{ valueFlags?: Set<string>; operands?: number }
> = {
	sudo: { valueFlags: new Set(["-u", "-g", "-p", "-C", "-h", "-U", "-T"]) },
	doas: { valueFlags: new Set(["-u", "-C"]) },
	env: {},
	command: {},
	builtin: {},
	exec: { valueFlags: new Set(["-a"]) },
	nohup: {},
	setsid: {},
	time: {},
	nice: { valueFlags: new Set(["-n"]) },
	ionice: { valueFlags: new Set(["-c", "-n", "-p"]) },
	stdbuf: { valueFlags: new Set(["-i", "-o", "-e"]) },
	// `timeout 30 cmd`: the duration is a positional operand, not a flag.
	timeout: { valueFlags: new Set(["-s", "-k"]), operands: 1 },
	xargs: {
		valueFlags: new Set(["-n", "-I", "-i", "-P", "-d", "-s", "-E", "-a"]),
	},
	// Package runners execute a real tool, and that tool is the subject.
	npx: { valueFlags: new Set(["-p", "--package", "-c", "--call"]) },
	bunx: { valueFlags: new Set(["-p", "--package"]) },
	pnpx: { valueFlags: new Set(["-p", "--package"]) },
	uvx: { valueFlags: new Set(["-p", "--python", "--with", "--from"]) },
};

/** Shells that take a command string to execute. */
const COMMAND_STRING_SHELLS = new Set(["bash", "sh", "zsh", "dash", "ksh"]);

/** `ssh` flags that consume the following argument. */
const SSH_VALUE_FLAGS = new Set([
	"-b",
	"-c",
	"-D",
	"-E",
	"-e",
	"-F",
	"-I",
	"-i",
	"-J",
	"-L",
	"-l",
	"-m",
	"-O",
	"-o",
	"-p",
	"-Q",
	"-R",
	"-S",
	"-W",
	"-w",
]);

type Word = { value: string; opaque: boolean };

/** Skip a double-quoted run, returning the index just past the closing quote. */
function skipDoubleQuoted(source: string, start: number): number {
	let i = start;
	while (i < source.length && source[i] !== '"') {
		i += source[i] === "\\" ? 2 : 1;
	}
	return i + 1;
}

/** Skip a balanced construct such as `$( ... )`, ignoring quoted delimiters. */
function skipBalanced(
	source: string,
	start: number,
	open: string,
	close: string,
): number {
	let depth = 1;
	let i = start;
	while (i < source.length && depth > 0) {
		const ch = source[i];
		if (ch === "\\") {
			i += 2;
			continue;
		}
		if (ch === "'") {
			const end = source.indexOf("'", i + 1);
			i = end === -1 ? source.length : end + 1;
			continue;
		}
		if (ch === '"') {
			i = skipDoubleQuoted(source, i + 1);
			continue;
		}
		if (ch === open) depth += 1;
		else if (ch === close) depth -= 1;
		i += 1;
	}
	return i;
}

/**
 * Split a command line into words grouped by statement separator.
 *
 * Quoting, command substitution and heredoc bodies are consumed as opaque
 * regions so operators inside them never split the enclosing command.
 */
function tokenize(source: string): Word[][] {
	const groups: Word[][] = [];
	let current: Word[] = [];
	let value = "";
	let started = false;
	let opaque = false;
	let i = 0;

	const endWord = () => {
		if (!started) return;
		current.push({ value, opaque });
		value = "";
		started = false;
		opaque = false;
	};

	const endGroup = () => {
		endWord();
		if (current.length > 0) groups.push(current);
		current = [];
	};

	/** Consume and discard a redirection target such as `file`, `&1` or `&-`. */
	const skipRedirectTarget = () => {
		while (i < source.length && (source[i] === " " || source[i] === "\t")) {
			i += 1;
		}
		if (source[i] === "&") {
			i += 1;
			while (i < source.length && /[\d-]/.test(source[i])) i += 1;
			return;
		}
		while (i < source.length && !/[\s;|&<>()]/.test(source[i])) {
			if (source[i] === "'") {
				const end = source.indexOf("'", i + 1);
				i = end === -1 ? source.length : end + 1;
				continue;
			}
			if (source[i] === '"') {
				i = skipDoubleQuoted(source, i + 1);
				continue;
			}
			i += 1;
		}
	};

	/** Consume a heredoc introduced at `i`, including its body. */
	const skipHeredoc = () => {
		i += 2;
		if (source[i] === "<") {
			// `<<<` is a here-string: only its operand is consumed.
			i += 1;
			skipRedirectTarget();
			return;
		}
		if (source[i] === "-") i += 1;
		while (i < source.length && (source[i] === " " || source[i] === "\t")) {
			i += 1;
		}
		let delimiter = "";
		while (i < source.length && !/[\s;|&<>()]/.test(source[i])) {
			const ch = source[i];
			if (ch === "'" || ch === '"') {
				const end = source.indexOf(ch, i + 1);
				const stop = end === -1 ? source.length : end;
				delimiter += source.slice(i + 1, stop);
				i = stop + 1;
				continue;
			}
			delimiter += ch;
			i += 1;
		}
		const lineEnd = source.indexOf("\n", i);
		if (delimiter === "" || lineEnd === -1) return;
		let cursor = lineEnd + 1;
		while (cursor <= source.length) {
			const next = source.indexOf("\n", cursor);
			const stop = next === -1 ? source.length : next;
			if (source.slice(cursor, stop).trim() === delimiter) {
				i = stop;
				return;
			}
			if (next === -1) break;
			cursor = next + 1;
		}
		i = source.length;
	};

	while (i < source.length) {
		const ch = source[i];

		if (ch === "#" && !started) {
			while (i < source.length && source[i] !== "\n") i += 1;
			continue;
		}

		if (ch === "\\") {
			const next = source[i + 1];
			if (next === undefined) break;
			if (next !== "\n") {
				value += next;
				started = true;
			}
			i += 2;
			continue;
		}

		if (ch === "'") {
			const end = source.indexOf("'", i + 1);
			const stop = end === -1 ? source.length : end;
			value += source.slice(i + 1, stop);
			started = true;
			i = stop + 1;
			continue;
		}

		if (ch === '"') {
			i += 1;
			while (i < source.length && source[i] !== '"') {
				if (source[i] === "\\" && i + 1 < source.length) {
					value += source[i + 1];
					i += 2;
					continue;
				}
				value += source[i];
				i += 1;
			}
			started = true;
			i += 1;
			continue;
		}

		if (ch === "$" && source[i + 1] === "(") {
			i = skipBalanced(source, i + 2, "(", ")");
			started = true;
			opaque = true;
			continue;
		}

		if (ch === "`") {
			const end = source.indexOf("`", i + 1);
			i = end === -1 ? source.length : end + 1;
			started = true;
			opaque = true;
			continue;
		}

		if (ch === "<" && source[i + 1] === "<") {
			endWord();
			skipHeredoc();
			continue;
		}

		if (ch === ">" || ch === "<") {
			// A bare file descriptor such as the `2` of `2>&1` is not a command.
			if (started && /^\d+$/.test(value)) {
				value = "";
				started = false;
			}
			endWord();
			i += 1;
			if (source[i] === ">" || source[i] === "&") i += 1;
			skipRedirectTarget();
			continue;
		}

		if (ch === "&" && source[i + 1] === ">") {
			endWord();
			i += 2;
			skipRedirectTarget();
			continue;
		}

		if (ch === ";" || ch === "&" || ch === "|" || ch === "\n") {
			endGroup();
			i += 1;
			if (
				(ch === ";" && source[i] === ";") ||
				(ch === "&" && source[i] === "&") ||
				(ch === "|" && source[i] === "|")
			) {
				i += 1;
			}
			continue;
		}

		if (ch === "(" || ch === ")") {
			endGroup();
			i += 1;
			continue;
		}

		if (ch === " " || ch === "\t" || ch === "\r") {
			endWord();
			i += 1;
			continue;
		}

		value += ch;
		started = true;
		i += 1;
	}

	endGroup();
	return groups;
}

function isAssignment(word: string): boolean {
	return /^[A-Za-z_][A-Za-z0-9_]*(\[[^\]]*\])?\+?=/.test(word);
}

function basename(command: string): string {
	const cleaned = command.replace(/\/+$/, "");
	const slash = cleaned.lastIndexOf("/");
	return slash === -1 ? cleaned : cleaned.slice(slash + 1);
}

/** Advance past a wrapper's own flags and, where applicable, its operands. */
function skipWrapperArguments(
	words: Word[],
	start: number,
	spec: { valueFlags?: Set<string>; operands?: number },
): number {
	let i = start;
	let operands = spec.operands ?? 0;
	while (i < words.length) {
		const word = words[i].value;
		if (word.startsWith("-") && word !== "-") {
			i += 1;
			if (spec.valueFlags?.has(word) && i < words.length) i += 1;
			continue;
		}
		if (isAssignment(word)) {
			i += 1;
			continue;
		}
		if (operands > 0) {
			operands -= 1;
			i += 1;
			continue;
		}
		break;
	}
	return i;
}

/** Resolve `ssh [flags] host [command]` into the invocation it really runs. */
function parseSsh(words: Word[], depth: number): ShellInvocation[] {
	let i = 1;
	while (i < words.length) {
		const word = words[i].value;
		if (!word.startsWith("-") || word === "-") break;
		i += 1;
		if (SSH_VALUE_FLAGS.has(word) && i < words.length) i += 1;
	}
	const destination = words[i];
	if (!destination) return [{ name: "ssh", args: [] }];
	const host = destination.value.replace(/^[^@]*@/, "");
	const remote = words.slice(i + 1);
	if (remote.length === 0) {
		return [{ name: "ssh", args: [destination.value], remoteHost: host }];
	}
	// A single token holds a quoted remote script and must be parsed again;
	// separate tokens are already the remote argv.
	const inner =
		remote.length === 1 && !remote[0].opaque
			? parseInternal(remote[0].value, depth + 1).invocations
			: [
					{
						name: basename(remote[0].value),
						args: remote.slice(1).map((word) => word.value),
					},
				];
	if (inner.length === 0) {
		return [{ name: "ssh", args: [destination.value], remoteHost: host }];
	}
	return inner.map((invocation) => ({
		...invocation,
		remoteHost: invocation.remoteHost ?? host,
	}));
}

/** Build the invocations a single separated word group performs. */
function buildInvocations(words: Word[], depth: number): ShellInvocation[] {
	let i = 0;
	while (i < words.length && PREFIX_KEYWORDS.has(words[i].value)) i += 1;
	if (i < words.length && STATEMENT_KEYWORDS.has(words[i].value)) return [];
	while (i < words.length && isAssignment(words[i].value)) i += 1;
	if (i >= words.length) return [];

	const head = words[i];
	if (head.opaque) return [];
	const name = basename(head.value);
	const rest = words.slice(i + 1);

	if (depth < MAX_DEPTH) {
		if (name === "ssh") return parseSsh(words.slice(i), depth);

		if (name === "eval") {
			const script = rest.map((word) => word.value).join(" ");
			const inner = script.trim()
				? parseInternal(script, depth + 1).invocations
				: [];
			if (inner.length > 0) return inner;
		}

		if (COMMAND_STRING_SHELLS.has(name)) {
			const flag = rest.findIndex(
				(word) => /^-[a-z]*c$/.test(word.value) && !word.opaque,
			);
			const script = flag === -1 ? undefined : rest[flag + 1];
			if (script && !script.opaque) {
				const inner = parseInternal(script.value, depth + 1).invocations;
				if (inner.length > 0) return inner;
			}
		}

		const wrapper = WRAPPERS[name];
		if (wrapper) {
			const next = skipWrapperArguments(words, i + 1, wrapper);
			const inner = buildInvocations(words.slice(next), depth + 1);
			if (inner.length > 0) return inner;
		}
	}

	return [{ name, args: rest.map((word) => word.value) }];
}

function parseInternal(command: string, depth: number): ParsedShellCommand {
	if (depth > MAX_DEPTH) return { invocations: [], primary: null };
	const source = command.slice(0, MAX_SOURCE_LENGTH);
	const invocations = tokenize(source).flatMap((group) =>
		buildInvocations(group, depth),
	);
	return { invocations, primary: selectPrimary(invocations) };
}

/**
 * The command a reader would name when asked what the line does.
 *
 * Setup commands (`cd`, `export`) and narration (`echo`) are skipped, so
 * `cd repo && cargo test` is a test run and `df -h; rm x` stays a disk check.
 */
function selectPrimary(invocations: ShellInvocation[]): ShellInvocation | null {
	for (const invocation of invocations) {
		if (!INCIDENTAL_COMMANDS.has(invocation.name)) return invocation;
	}
	return invocations[0] ?? null;
}

export function parseShellCommand(command: string): ParsedShellCommand {
	return parseInternal(command, 0);
}

/**
 * First argument that is not a flag, e.g. the subcommand of `git status`.
 *
 * `valueFlags` names the flags that consume the argument after them, so
 * `tail -n 50 log.txt` reports the file rather than the line count.
 */
export function firstOperand(
	args: string[],
	valueFlags?: ReadonlySet<string>,
): string | null {
	for (let i = 0; i < args.length; i += 1) {
		const arg = args[i];
		if (!arg.startsWith("-") || arg === "-") {
			if (arg === "-") continue;
			return arg;
		}
		if (valueFlags?.has(arg)) i += 1;
	}
	return null;
}
