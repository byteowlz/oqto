import {
	type Context,
	defineService,
	type ReplicatedState,
} from "@earendil-works/chord";

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
export interface Permissions {
	readonly state: ReplicatedState<PermissionState>;
	decide(decision: PermissionDecision, context: Context): Promise<void>;
	revokeAll(context: Context): Promise<void>;
}
export const Permissions = defineService<Permissions>("oq.permissions.demo.v1");
