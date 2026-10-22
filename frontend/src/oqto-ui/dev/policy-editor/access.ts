import {
	type Draft,
	type GrantMode,
	type Operation,
	type Resource,
	byId,
	connections,
	operationsFor,
	workspaceOf,
} from "./resources";

/** Example ceilings so the mockup exercises locks. Not a policy evaluator. */
export function lockReason(
	subject: string,
	resource: Resource,
	operation: Operation,
): string | undefined {
	if (resource.kind === "service") {
		if (operation.ownerOnly) return "Owner-controlled · cannot delegate";
		if (workspaceOf(subject) === "ws-client" && resource.id === "svc-ssh")
			return "Client policy · no remote connectors";
		return undefined;
	}
	if (resource.kind !== "wiki" && resource.kind !== "memory") {
		return resource.id === subject
			? undefined
			: "Other workdir · no cross-access";
	}
	if (operation.ownerOnly) return "Owner-controlled · cannot delegate";
	const space = workspaceOf(subject);
	const clientData = resource.id.endsWith("-client");
	if (space === "ws-client" && !clientData)
		return "Client policy · no external knowledge";
	if (clientData && space !== "ws-client")
		return "Client data stays in client workdirs";
	if (
		(resource.id === "wiki-private" || resource.id === "mem-personal") &&
		space !== "ws-personal"
	)
		return "Personal data cannot enter shared workspaces";
	return undefined;
}

export function currentAccess(
	subject: string,
	resource: Resource,
	operation: Operation,
): GrantMode {
	if (lockReason(subject, resource, operation)) return "deny";
	if (resource.id === subject)
		return operation.id === "execute" ? "ask" : "allow";
	if (resource.kind === "service") {
		if (!connections[subject]?.includes(resource.id)) return "deny";
		return operation.id === "use" ? (resource.use ?? "ask") : "deny";
	}
	if (!connections[subject]?.includes(resource.id)) return "deny";
	return operation.id === "read" ? "allow" : "ask";
}

export interface OperationView {
	id: string;
	labelKey: string;
	shortKey: string;
	current: GrantMode;
	proposed?: GrantMode;
	lockKey?: string;
}
export function operationViews(
	subject: string,
	resource: Resource,
	drafts: readonly Draft[],
): OperationView[] {
	return operationsFor(resource.kind).map((operation) => ({
		id: operation.id,
		labelKey: operation.labelKey,
		shortKey: operation.shortKey,
		current: currentAccess(subject, resource, operation),
		proposed: drafts.find(
			(draft) =>
				draft.subject === subject &&
				draft.resource === resource.id &&
				draft.operation === operation.id,
		)?.mode,
		lockKey: lockReason(subject, resource, operation),
	}));
}

/** A draft equal to current access is no change, so it is removed. */
export function propose(
	drafts: readonly Draft[],
	next: Draft,
): readonly Draft[] {
	const resource = byId(next.resource);
	const operation = resource
		? operationsFor(resource.kind).find((item) => item.id === next.operation)
		: undefined;
	if (!resource || !operation || lockReason(next.subject, resource, operation))
		return drafts;
	const rest = drafts.filter(
		(draft) =>
			!(
				draft.subject === next.subject &&
				draft.resource === next.resource &&
				draft.operation === next.operation
			),
	);
	return currentAccess(next.subject, resource, operation) === next.mode
		? rest
		: [...rest, next];
}
