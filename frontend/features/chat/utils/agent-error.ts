/**
 * Turn a raw agent failure into something a person can act on.
 *
 * The runner sends the tail of the process's stderr, so the input is typically a
 * runtime stack trace. The line that matters is almost never the outermost
 * frame: it is the one naming the denied path, the failed allocation or the
 * unreachable endpoint. Pull that out, and keep the rest for whoever wants it.
 */

export type AgentErrorKind =
	| "sandbox-denied"
	| "allocation-failed"
	| "unreachable"
	| "exited"
	| "generic";

export interface AgentErrorDescription {
	kind: AgentErrorKind;
	/** Short, plain statement of what happened. */
	headline: string;
	/** The single most informative line, when one stands out. */
	cause?: string;
	/** Raw text, shown only on request. Absent when it adds nothing. */
	detail?: string;
}

const PROCESS_EXITED = /Agent process exited/i;
const STDERR_MARKER = /Last stderr output:\s*/i;

/** A denied path is the whole diagnosis, so keep the path itself. */
const DENIED = /\b(EACCES|EPERM|permission denied|Operation not permitted)\b/i;
const DENIED_PATH = /['"]([^'"]*\/[^'"]*)['"]/;

/**
 * Address space exhaustion reads as a memory error but is usually a limit on
 * reserved space rather than a shortage of memory.
 */
const ALLOCATION =
	/\b(could not allocate|cannot allocate|out of memory|ENOMEM|RangeError)\b/i;

const UNREACHABLE =
	/\b(ECONNREFUSED|ENOTFOUND|EHOSTUNREACH|ENETUNREACH|ETIMEDOUT|fetch failed|Connection refused)\b/i;

/** Frames and echoed source carry no diagnosis; the message line does. */
function isNoise(line: string): boolean {
	const trimmed = line.trim();
	if (!trimmed) return true;
	if (/^at\s/.test(trimmed)) return true;
	if (/^\^+$/.test(trimmed)) return true;
	if (/^Node\.js v/.test(trimmed)) return true;
	return false;
}

function significantLines(raw: string): string[] {
	return raw.split("\n").filter((line) => !isNoise(line));
}

/**
 * Prefer a line that names a recognised failure; such a line explains the
 * failure on its own, whereas the first surviving line often does not.
 */
function pickCause(raw: string): string | undefined {
	const lines = significantLines(raw);
	const telling = lines.find(
		(line) =>
			DENIED.test(line) || ALLOCATION.test(line) || UNREACHABLE.test(line),
	);
	const chosen = telling ?? lines[0];
	return chosen?.trim() || undefined;
}

export function describeAgentError(raw: string): AgentErrorDescription {
	const text = (raw ?? "").trim();
	if (!text) {
		return { kind: "generic", headline: "Something went wrong." };
	}

	const exited = PROCESS_EXITED.test(text);
	// Everything after the marker is the captured output rather than our prose.
	const body = text.split(STDERR_MARKER)[1]?.trim() ?? "";
	const searchable = body || text;
	const cause = pickCause(searchable);
	// Showing prose we wrote ourselves back to the user as "detail" is noise.
	const detail = body || (exited ? undefined : text);

	if (DENIED.test(searchable)) {
		const path = searchable.match(DENIED_PATH)?.[1];
		return {
			kind: "sandbox-denied",
			headline: path
				? `The agent was denied access to ${path}`
				: "The agent was denied access to a file it needs.",
			cause,
			detail,
		};
	}

	if (ALLOCATION.test(searchable)) {
		return {
			kind: "allocation-failed",
			headline: "The agent could not allocate memory and stopped.",
			cause,
			detail,
		};
	}

	if (UNREACHABLE.test(searchable)) {
		return {
			kind: "unreachable",
			headline: "The agent could not reach a service it needs.",
			cause,
			detail,
		};
	}

	if (exited) {
		return {
			kind: "exited",
			headline: "The agent stopped unexpectedly.",
			cause,
			detail,
		};
	}

	// Short messages are already the headline; repeating them helps nobody.
	const oneLine = !text.includes("\n") && text.length <= 160;
	return {
		kind: "generic",
		headline: oneLine ? text : "The agent reported an error.",
		cause: oneLine ? undefined : cause,
		detail: oneLine ? undefined : text,
	};
}
