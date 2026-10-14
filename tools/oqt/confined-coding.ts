import { resolve } from "node:path";
import type { Context } from "@earendil-works/chord";
import type { EnvTarget } from "@earendil-works/pi-durable";
import { createRegistry } from "@earendil-works/pi-durable";
import { createCodingTools } from "@earendil-works/pi-durable/tools";
import { Connection, RemoteExecutionEnv } from "@earendil-works/pi-env";
import { type CodingGate, gatedTool } from "./gated-tool.ts";

/** Trusted owner composition only. This is isolation, not ledger admission. */
export function confinedCoding(options: {
	workspace: string;
	launcher: string;
	spec: string;
	gate?: CodingGate;
	publicSession?: string;
}) {
	if ((options.gate === undefined) !== (options.publicSession === undefined))
		throw new Error(
			"Gate and owner-bound public Session must be supplied together",
		);
	const workspace = resolve(options.workspace);
	const connection = new Connection({
		command: [options.launcher, options.spec],
	});
	const execution = new RemoteExecutionEnv({
		connection,
		id: "oqt-confined-workspace",
		cwd: workspace,
	});
	const registry = createRegistry();
	const tools = createCodingTools();
	registry.install(
		options.gate && options.publicSession
			? {
					...tools,
					tools: tools.tools?.map((tool) =>
						gatedTool(
							tool,
							options.gate as CodingGate,
							options.publicSession as string,
						),
					),
				}
			: tools,
	);
	return {
		registry,
		env(target: EnvTarget) {
			// A conversation/model may not select a different host work directory.
			if (target.cwd === undefined || resolve(target.cwd) !== workspace)
				throw new Error(
					"Conversation work directory is outside the owner-bound executor",
				);
			return execution;
		},
		async cleanup(context: Context) {
			try {
				await execution.cleanup(context);
			} finally {
				connection.close();
			}
		},
	};
}
