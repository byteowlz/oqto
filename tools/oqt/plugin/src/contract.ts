import {
	type Context,
	defineService,
	type ReplicatedState,
} from "@earendil-works/chord";

export interface PendingPermission {
	readonly id: string;
	readonly digest: string;
	readonly revision: number;
	readonly capability: string;
	readonly resource: string;
	readonly argumentsText: string;
	readonly session: string;
}
export interface PermissionState {
	readonly pending: readonly PendingPermission[];
}
export interface PermissionDecision {
	readonly id: string;
	readonly digest: string;
	readonly revision: number;
	readonly allow: boolean;
	readonly lifetime: "once" | "session";
}
export interface Permissions {
	readonly state: ReplicatedState<PermissionState>;
	decide(decision: PermissionDecision, context: Context): Promise<void>;
	revokeAll(context: Context): Promise<void>;
}
export const Permissions = defineService<Permissions>("oq.permissions.demo.v1");
