import { appendFileSync } from "node:fs";

/** OQT_DEBUG startup trace: `<epoch ms> <pid> <label>` lines in one file
 * shared by launcher, host and guard (the confined brain logs to brain.log). */
const file = process.env.OQT_DEBUG ? process.env.OQT_STARTUP_TRACE : undefined;

export function mark(label: string, at = Date.now()): void {
	if (!file) return;
	try {
		appendFileSync(file, `${Math.round(at)} ${process.pid} ${label}\n`, {
			mode: 0o600,
		});
	} catch {}
}
