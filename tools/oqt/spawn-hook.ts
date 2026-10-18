// Copied beside upstream's session-worker manager. In coding mode every
// session worker is a confined brain launched by the guard; there is no
// unconfined fallback. The synthetic demo keeps upstream's spawn.
import type { ChildProcess } from "node:child_process";
import type { Guard } from "./guard.ts";
import type { InputAdmission, NativeInputCall } from "./input-leases.ts";
import {
	type InternalProcessSpawnOptions,
	spawnInternalProcess,
} from "./process.ts";

let active: Guard | undefined;

export function installGuard(guard: Guard | undefined) {
	active = guard;
}

export function registerOqtInputAttachment(session: string, attachment: string, cancel: (id: string) => Promise<void>, stop: () => Promise<void>) {
	if (active) active.input(session).register(attachment, cancel, stop);
	else if (process.env.OQT_MODE === "coding") throw new Error("Input guard unavailable");
}
export function forgetOqtInputAttachment(session: string, attachment: string) {
	if (active) active.input(session).forget(attachment);
}
export async function dropOqtInputAttachment(session: string, attachment: string) {
	if (active) await active.input(session).unregister(attachment);
}
export async function beforeOqtInputCall(session: string, attachment: string, call: NativeInputCall): Promise<InputAdmission | undefined> {
	if (active) return active.input(session).before(attachment, call);
	if (process.env.OQT_MODE === "coding") throw new Error("Input guard unavailable");
}
export function assertOqtInputCall(session: string, admission: InputAdmission | undefined) {
	if (active) active.input(session).assertCurrent(admission);
}
export async function afterOqtInputCall(session: string, admission: InputAdmission | undefined, call: NativeInputCall, result: unknown) {
	if (active) await active.input(session).after(admission, call, result);
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
