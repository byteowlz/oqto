// OSC 7501 program status for the oqt TUI (spec:
// https://www.superlogical.com/rex/docs/build/program-status). Pi's terminal
// writes it only when the terminal answered the support query, or when
// PI_PROGRAM_STATUS=1 forces it. Status is presentation only: it never grants,
// approves or selects anything. Messages are limited to dialog headings and the
// first line of errors; prompts, arguments and assistant output are never sent.
import type { ProgramStatus, Terminal } from "@earendil-works/pi-tui";

export type BlockedStatus = {
	kind: NonNullable<ProgramStatus["kind"]>;
	message: string;
};

type Entry = {
	model?: readonly {
		role?: string;
		stopReason?: string;
		errorMessage?: string;
	}[];
};

const firstLine = (text: string | undefined) =>
	text?.split(/\r?\n/, 1)[0]?.trim() || "Error";

/** How the latest run ended, from the conversation's committed entries. */
export function runOutcome(entries: readonly Entry[]): ProgramStatus {
	for (let index = entries.length - 1; index >= 0; index--) {
		const message = entries[index]?.model?.find(
			(candidate) => candidate.role === "assistant",
		);
		if (!message) continue;
		if (message.stopReason === "error")
			return { state: "error", message: firstLine(message.errorMessage) };
		if (message.stopReason === "aborted") return { state: "idle" };
		return { state: "done" };
	}
	return { state: "idle" };
}

export class OqtProgramStatus {
	#terminal: Terminal | undefined;
	#running = false;
	#resting: ProgramStatus = { state: "idle" };
	/** Open dialogs by source, in opening order; the most recent is reported. */
	readonly #blocked = new Map<string, BlockedStatus>();
	#last: string | undefined;

	attach(terminal: Terminal): void {
		this.#terminal = terminal;
		this.#last = undefined;
		this.#report();
	}

	/** Clears the status (on exit) so the terminal does not show a stale state. */
	detach(): void {
		this.#terminal?.setProgramStatus({ state: "clear" });
		this.#terminal = undefined;
		this.#last = undefined;
	}

	/** Called on every view change: a run is active, or how the last one ended. */
	observe(active: boolean, entries: readonly Entry[]): void {
		if (active) this.#running = true;
		else if (this.#running) {
			this.#running = false;
			this.#resting = runOutcome(entries);
		}
		this.#report();
	}

	/** `blocked` for a dialog until cleared with `undefined`. */
	setBlocked(source: string, status: BlockedStatus | undefined): void {
		this.#blocked.delete(source);
		if (status) this.#blocked.set(source, status);
		this.#report();
	}

	current(): ProgramStatus {
		const blocked = [...this.#blocked.values()].at(-1);
		if (blocked) return { state: "blocked", ...blocked, app: "oqt" };
		return {
			...(this.#running ? { state: "working" } : this.#resting),
			app: "oqt",
		};
	}

	#report(): void {
		if (!this.#terminal) return;
		const status = this.current();
		const key = JSON.stringify(status);
		if (key === this.#last) return;
		this.#last = key;
		this.#terminal.setProgramStatus(status);
	}
}

/** One TUI per process: the client renders, the permission plugin blocks. The
 * plugin is bundled separately (its own module copy), so the instance is shared
 * through a process-global symbol rather than module identity. */
const SHARED = Symbol.for("oqt.program-status.v1");
const holder = globalThis as { [SHARED]?: OqtProgramStatus };
holder[SHARED] ??= new OqtProgramStatus();
export const programStatus: OqtProgramStatus = holder[SHARED];
