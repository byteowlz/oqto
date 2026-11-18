import { Type } from "@earendil-works/pi-ai";
import type { ToolRegistration } from "@earendil-works/pi-durable";

/** A named machine as the guard describes it to the brain: a name and its
 * platform, never an address, key or limit (ADR-0086). */
export interface MachineInfo {
	machine: string;
	platform: string | null;
	cwd: string | null;
}

/** What the owner means by "machines", in every coding tool's description so
 * "run it on the Linux box" makes sense even before any machine is added. */
export const MACHINES_NOTE =
	"Machines are the owner's computers that can run this tool; the owner adds and manages them with /machines (alias /hands). Never reach another computer any other way, such as ssh.";

/** Add the optional `machine` argument to a coding tool. The guard sees it
 * in the admitted operation and routes the call; the upstream tool receives
 * its own arguments unchanged. No machines configured: only the note is
 * added, so the agent can tell the owner none is set up. */
export function withMachine(
	tool: ToolRegistration,
	machines: readonly MachineInfo[],
): ToolRegistration {
	if (machines.length === 0)
		return {
			...tool,
			description: `${tool.description}\n${MACHINES_NOTE} None is set up now, so this runs on this machine; if the owner asks for another machine, say so and point to /machines.`,
		};
	const parameters = tool.parameters as unknown as {
		properties?: Record<string, unknown>;
		additionalProperties?: boolean;
	};
	const list = machines
		.map((m) => `${m.machine}${m.platform ? ` (${m.platform})` : ""}`)
		.join(", ");
	return {
		...tool,
		description: `${tool.description}\n${MACHINES_NOTE} Optional machine: run this call on a named machine's confined hands (${list}); omit it for this machine. If the owner describes a machine ("on the Linux box") and it is unclear which one, ask. Paths and commands then refer to that machine's workspace. There is no fallback: an unknown or unavailable machine fails the call without running it.`,
		parameters: Type.Object(
			{
				...(parameters.properties ?? {}),
				machine: Type.Optional(
					Type.String({
						description: `Named machine (${list}); omit for this machine`,
					}),
				),
			} as Parameters<typeof Type.Object>[0],
			parameters.additionalProperties === undefined
				? {}
				: { additionalProperties: parameters.additionalProperties },
		) as ToolRegistration["parameters"],
		execute(args, api, context) {
			const { machine: _machine, ...rest } = (args ?? {}) as Record<
				string,
				unknown
			>;
			return tool.execute(rest as typeof args, api, context);
		},
	};
}
