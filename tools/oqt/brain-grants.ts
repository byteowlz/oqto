/** Trusted, in-process service authorization. Not a process sandbox or a store.
 * Keep the controller in the supervisor; give tools only a bound client. Store
 * adapters must filter by `resources` before search/ranking/counts and honor the
 * signal. Never give an adapter an unrestricted store handle via model args.
 * This module is not yet registered with oqt's coding harness.
 */
export type BrainOperation =
	| "history.search"
	| "history.read"
	| "memory.search"
	| "memory.read"
	| "memory.record";
export interface BrainGrant {
	readonly operation: BrainOperation;
	readonly resource: string;
	readonly delegate?: boolean;
}
export interface BrainIdentity {
	readonly session: string;
	readonly conversation: string;
}
export interface BrainScope extends BrainIdentity {
	readonly operation: BrainOperation;
	readonly resources: readonly string[];
	readonly signal: AbortSignal;
}
export interface BrainClient {
	/** Run only a trusted adapter, not a callback provided by the model.
	 * Omitted resources means all permitted resources, never a global scope.
	 * Explicit scopes must be wholly authorized, not silently truncated.
	 */
	run<T>(
		operation: BrainOperation,
		resources: readonly string[] | undefined,
		adapter: (scope: BrainScope) => Promise<T>,
	): Promise<T>;
}
export class BrainAccessDenied extends Error {
	constructor() {
		super("Brain resource access denied");
		this.name = "BrainAccessDenied";
	}
}
const operations = new Set<BrainOperation>([
	"history.search",
	"history.read",
	"memory.search",
	"memory.read",
	"memory.record",
]);
function identifier(value: string) {
	if (
		typeof value !== "string" ||
		!value.length ||
		value.length > 512 ||
		[...value].some((character) => character.charCodeAt(0) < 32)
	)
		throw new Error("Invalid brain resource identity");
	return value;
}
function copyGrants(grants: readonly BrainGrant[]): readonly BrainGrant[] {
	if (grants.length > 4096) throw new Error("Brain grant limit exceeded");
	const seen = new Set<string>();
	return Object.freeze(
		grants.map((grant) => {
			if (
				!operations.has(grant.operation) ||
				(grant.delegate !== undefined && typeof grant.delegate !== "boolean")
			)
				throw new Error("Invalid brain grant");
			const resource = identifier(grant.resource);
			const key = JSON.stringify([grant.operation, resource]);
			if (seen.has(key)) throw new Error("Duplicate brain grant");
			seen.add(key);
			return Object.freeze({
				operation: grant.operation,
				resource,
				delegate: grant.delegate === true,
			});
		}),
	);
}
interface Binding {
	readonly identity: BrainIdentity;
	readonly parent?: Binding;
	grants: readonly BrainGrant[];
	revision: number;
	closed: boolean;
	readonly active: Set<{ abort: AbortController; done: Promise<void> }>;
}

/** Bootstrap under supervisor authority. Object identity, not caller-supplied
 * session IDs, selects a binding. No controller method is placed on the client.
 * Updates publish synchronously; their Promise acknowledges drained old calls.
 * Do not await update from inside an adapter belonging to the affected subtree.
 */
export function createBrainGrantController() {
	const bindings = new WeakMap<BrainClient, Binding>();
	const all = new Set<Binding>();
	function lookup(client: BrainClient) {
		const binding = bindings.get(client);
		if (!binding || binding.closed) throw new BrainAccessDenied();
		return binding;
	}
	function chain(binding: Binding): Binding[] {
		return binding.parent ? [binding, ...chain(binding.parent)] : [binding];
	}
	function effective(binding: Binding): readonly BrainGrant[] {
		if (chain(binding).some((ancestor) => ancestor.closed)) return [];
		const parent = binding.parent;
		if (!parent) return binding.grants;
		const ceiling = effective(parent).filter((grant) => grant.delegate);
		return binding.grants.filter((grant) =>
			ceiling.some(
				(allowed) =>
					allowed.operation === grant.operation &&
					allowed.resource === grant.resource,
			),
		);
	}
	function bind(
		identity: BrainIdentity,
		grants: readonly BrainGrant[],
		parent?: Binding,
	): BrainClient {
		if (all.size >= 4096 || (parent && chain(parent).length >= 32))
			throw new Error("Brain binding limit exceeded");
		const binding: Binding = {
			identity: Object.freeze({
				session: identifier(identity.session),
				conversation: identifier(identity.conversation),
			}),
			grants: copyGrants(grants),
			parent,
			revision: 0,
			closed: false,
			active: new Set(),
		};
		const client: BrainClient = Object.freeze({
			async run<T>(
				operation: BrainOperation,
				requested: readonly string[] | undefined,
				adapter: (scope: BrainScope) => Promise<T>,
			): Promise<T> {
				lookup(client);
				const allowed = new Set(
					effective(binding)
						.filter((grant) => grant.operation === operation)
						.map((grant) => grant.resource),
				);
				const resources =
					requested === undefined ? [...allowed] : [...requested];
				if (
					!operations.has(operation) ||
					!resources.length ||
					resources.length > 4096 ||
					resources.some((resource) => !allowed.has(resource))
				)
					throw new BrainAccessDenied();
				const ancestors = chain(binding);
				const revisions = ancestors.map((ancestor) => ancestor.revision);
				const abort = new AbortController();
				let finish: () => void = () => {};
				const done = new Promise<void>((resolve) => {
					finish = resolve;
				});
				const active = { abort, done };
				binding.active.add(active);
				try {
					const result = await adapter(
						Object.freeze({
							...binding.identity,
							operation,
							resources: Object.freeze([...new Set(resources)].sort()),
							signal: abort.signal,
						}),
					);
					if (
						abort.signal.aborted ||
						ancestors.some(
							(ancestor, index) =>
								ancestor.closed || ancestor.revision !== revisions[index],
						)
					)
						throw new BrainAccessDenied();
					return result;
				} catch (error) {
					if (
						abort.signal.aborted ||
						ancestors.some(
							(ancestor, index) =>
								ancestor.closed || ancestor.revision !== revisions[index],
						)
					)
						throw new BrainAccessDenied();
					throw error;
				} finally {
					binding.active.delete(active);
					finish();
				}
			},
		});
		bindings.set(client, binding);
		all.add(binding);
		return client;
	}
	function drain(binding: Binding) {
		const pending: Promise<void>[] = [];
		for (const descendant of all) {
			if (!chain(descendant).includes(binding)) continue;
			for (const active of descendant.active) {
				pending.push(active.done);
				active.abort.abort();
			}
		}
		return Promise.all(pending).then(() => undefined);
	}
	return Object.freeze({
		create(identity: BrainIdentity, grants: readonly BrainGrant[]) {
			return bind(identity, grants);
		},
		child(
			parent: BrainClient,
			identity: BrainIdentity,
			grants: readonly BrainGrant[],
		) {
			return bind(identity, grants, lookup(parent));
		},
		update(client: BrainClient, grants: readonly BrainGrant[]) {
			const binding = lookup(client);
			binding.grants = copyGrants(grants);
			binding.revision++;
			return drain(binding);
		},
		close(client: BrainClient) {
			const binding = lookup(client);
			binding.closed = true;
			binding.revision++;
			const drained = drain(binding);
			for (const descendant of all) {
				if (chain(descendant).includes(binding)) {
					descendant.closed = true;
					all.delete(descendant);
				}
			}
			return drained;
		},
	});
}
