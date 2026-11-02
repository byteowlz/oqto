/** Confined-brain launcher for host2: every coding Session worker is a brain
 * the guard starts inside oqt_brain. There is no unconfined fallback; the
 * worker socket directory is bound read-only so the brain can only connect.
 */
import { dirname } from "node:path";
import type { Guard } from "../experimental/guard.ts";
import type { WorkerLauncher } from "./server.ts";
import { INIT_ENV } from "./worker.ts";

export function guardLauncher(
	guard: Guard,
	options: {
		/** host2/brain-worker.ts inside a trusted read-only brain bind. */
		readonly entry: string;
		/** The Session's workspace (hands execution directory). */
		readonly workspace: (session: string) => string;
	},
): WorkerLauncher {
	return (init) => {
		const cwd = options.workspace(init.session);
		const child = guard.spawnBrain(
			{
				metadata: { id: init.session, cwd, path: init.database },
				pluginManifestPaths: [],
			},
			{ [INIT_ENV]: JSON.stringify({ ...init, cwd }) },
			{ entry: options.entry, bind: [dirname(init.socket)] },
		);
		const exited = new Promise<unknown>((resolve) => {
			if (child.exitCode !== null || child.signalCode !== null)
				resolve(child.exitCode);
			else child.once("exit", resolve);
		});
		return {
			pid: child.pid,
			exited,
			kill() {
				// The guard spawns bwrap detached: its process group is the whole brain.
				try {
					if (child.pid) process.kill(-child.pid, "SIGKILL");
				} catch {
					child.kill("SIGKILL");
				}
			},
		};
	};
}
