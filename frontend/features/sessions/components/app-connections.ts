import { invokeAppConnection } from "@/lib/api/apps";
import type { OqtoConnectionsCapability } from "@byteowlz/oqto-app-sdk";

/** The frame may choose only slot/path. Mount authority stays in this closure. */
export function createConnectionsCapability(
	workspacePath: string,
	instanceId: string,
): OqtoConnectionsCapability {
	return {
		invoke: (slot, path) =>
			invokeAppConnection(workspacePath, instanceId, slot, path),
	};
}
