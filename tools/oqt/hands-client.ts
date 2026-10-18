import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Connection, RemoteExecutionEnv } from "@earendil-works/pi-env";

/** Brain-side view of guard-owned hands. Holds no authority: the guard names
 * the current epoch; a new epoch always gets fresh Connection/env objects and
 * the previous ones are closed, so stale handles cannot be revived.
 */
export class HandsClient {
	readonly socketPath: string;
	readonly workspace: string;
	readonly proxy: string;
	#epoch: string | undefined;
	#connection: Connection | undefined;
	#execution: RemoteExecutionEnv | undefined;
	#generation = 0;
	constructor(socketPath: string, workspace: string, proxy?: string) {
		this.socketPath = socketPath;
		this.workspace = resolve(workspace);
		this.proxy =
			proxy ?? fileURLToPath(new URL("./hands-proxy.mjs", import.meta.url));
	}
	use(epoch: string) {
		if (!/^[a-f0-9]{32}$/.test(epoch)) throw new Error("Invalid hands epoch");
		if (epoch === this.#epoch && this.#execution) return this.#execution;
		this.#connection?.close();
		this.#generation++;
		this.#epoch = epoch;
		this.#connection = new Connection({
			command: [process.execPath, this.proxy, this.socketPath, epoch],
		});
		this.#execution = new RemoteExecutionEnv({
			connection: this.#connection,
			id: `oqt-hands-${this.#generation}`,
			cwd: this.workspace,
		});
		return this.#execution;
	}
	/** Current env for the last epoch the guard admitted; throws before any admission. */
	get current() {
		if (!this.#execution) throw new Error("No admitted hands generation");
		return this.#execution;
	}
	close() {
		this.#connection?.close();
		this.#connection = undefined;
		this.#execution = undefined;
		this.#epoch = undefined;
	}
}
