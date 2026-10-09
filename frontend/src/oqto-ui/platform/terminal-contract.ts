/**
 * The terminal port OqtoUI's Terminal pane talks to. A session is a live
 * PTY: bytes out through the callback, keystrokes and size changes in. The
 * pane never learns how those bytes travel.
 */

export interface TerminalSize {
	readonly cols: number;
	readonly rows: number;
}

export interface TerminalSession {
	/** Keystrokes, exactly as the terminal produced them. */
	input(data: string): void;
	resize(size: TerminalSize): void;
	close(): void;
}

export interface TerminalHost {
	/**
	 * Starts a PTY in the work directory. `onOutput` receives raw bytes; the
	 * promise resolves once the host has opened the session.
	 */
	open(
		workspacePath: string,
		size: TerminalSize,
		onOutput: (bytes: Uint8Array) => void,
	): Promise<TerminalSession>;
}
