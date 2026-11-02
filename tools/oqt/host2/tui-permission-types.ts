// host2 copy of plugin/src/contract.ts (ADR-0071): identical logic, imports moved to the
// local TUI API. The pinned plugin copy is deleted with the pinned transport.
export interface PendingPermission {
	readonly id: string;
	readonly digest: string;
	readonly revision: number;
	/** Coding mode: workspace tool call or external resource request. */
	readonly kind?: "tool" | "resource";
	/** Coding mode: what approving would cover. */
	readonly category?: "read" | "edit" | "shell" | "resource";
	/** Simple shell commands: the prefix a session/always decision covers. */
	readonly prefix?: string;
	/** Human preview (command, diff, path) for the approval menu. */
	readonly preview?: string;
	readonly capability: string;
	readonly resource: string;
	readonly argumentsText: string;
	readonly session: string;
}
export interface PermissionState {
	readonly mode?: "demo" | "coding";
	readonly pending: readonly PendingPermission[];
}
export interface PermissionDecision {
	readonly id: string;
	readonly digest: string;
	readonly revision: number;
	readonly allow: boolean;
	readonly lifetime: "once" | "session";
	/** Coding mode: keep as a workspace rule ("always in this repo"). */
	readonly remember?: boolean;
}
