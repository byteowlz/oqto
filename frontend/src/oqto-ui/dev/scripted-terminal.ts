/**
 * Scripted terminal for /dev/oqto-ui: a session that echoes what is typed,
 * so the pane's wiring is exercised without a PTY behind it.
 */

import type { TerminalHost } from "../platform/terminal-contract";

export function createScriptedTerminalHost(): TerminalHost {
	return {
		async open(_workspacePath, _size, onOutput) {
			const encoder = new TextEncoder();
			onOutput(encoder.encode("scripted terminal\r\n$ "));
			return {
				input(data) {
					onOutput(encoder.encode(data));
				},
				resize() {},
				close() {},
			};
		},
	};
}
