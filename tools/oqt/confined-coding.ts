import { resolve } from "node:path";
import type { Context } from "@earendil-works/chord/context";
import type { EnvTarget } from "@earendil-works/pi-durable";
import { createRegistry } from "@earendil-works/pi-durable";
import { createCodingTools } from "@earendil-works/pi-durable/tools";
import { Connection, RemoteExecutionEnv } from "@earendil-works/pi-env";

/** Trusted owner composition only. This is isolation, not ledger admission. */
export function confinedCoding(options: {
	workspace: string;
	launcher: string;
	spec: string;
}) {
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
	registry.install(createCodingTools());
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
