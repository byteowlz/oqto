// Copied beside upstream's session-worker manager. In coding mode every
// session worker is a confined brain launched by the guard; there is no
// unconfined fallback. The synthetic demo keeps upstream's spawn.
import type { ChildProcess } from "node:child_process";
import type { Guard } from "./guard.ts";
import {
	type InternalProcessSpawnOptions,
	spawnInternalProcess,
} from "./process.ts";

let active: Guard | undefined;

export function installGuard(guard: Guard | undefined) {
	active = guard;
}

export function spawnOqtSessionWorker(
	options: {
		metadata: { id: string; cwd: string; path: string };
		pluginManifestPaths: readonly string[];
	},
	spawnOptions: InternalProcessSpawnOptions,
): ChildProcess {
	if (active) {
		try {
			return active.spawnBrain(
				options,
				Object.fromEntries(
					Object.entries(spawnOptions.env ?? {}).filter(
						(entry): entry is [string, string] => typeof entry[1] === "string",
					),
				),
			);
		} catch (error) {
			// Upstream reports worker startup failures only as an internal error;
			// keep the cause in the private host log.
			console.error("oqt: confined brain spawn refused:", error);
			throw error;
		}
	}
	if (process.env.OQT_MODE === "coding")
		throw new Error(
			"Coding sessions require the oqt guard; refusing an unconfined worker",
		);
	return spawnInternalProcess(
		"session-worker",
		[JSON.stringify(options)],
		spawnOptions,
	);
}
