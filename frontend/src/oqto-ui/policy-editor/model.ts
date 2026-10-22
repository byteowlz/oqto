export type Mode = "read" | "readwrite" | "deny" | "ask" | "inherit";
export interface ResolverDecision {
	access: "None" | "Read" | "Write";
	origin: {
		layer: "System" | "Admin" | "Workspace" | "Session";
		source: string;
	};
	matchedPath: string | null;
}
export interface PolicyNode {
	id: string;
	parentId: string | null;
	/** Filesystem display name, not translated UI copy. */
	name: string;
	path: string;
	effective: Exclude<Mode, "inherit">;
	/** Optional host-derived display scope; never inferred from resolver layer. */
	origin?: "owner/global" | "repo";
	inherited: boolean;
	mixed: boolean;
	locked: boolean;
	containsLockedDescendant: boolean;
	provenance: {
		decisive: readonly ResolverDecision[];
		decisions: readonly ResolverDecision[];
	};
	lockReason?: string;
	scope?: string;
	duration?: string;
	allowedChoices: readonly Mode[];
	workDirectoryId?: string;
}
export interface Snapshot {
	/** Must be supplied by the trusted projection mapper; never inferred from access modes. */
	projection: "containment_only" | "containment_and_guard";
	identity: string;
	revision: string;
	nodes: readonly PolicyNode[];
}
export interface Edit {
	nodeId: string;
	mode: Mode;
	source: "operator" | "interviewer";
}
export interface Binding {
	identity: string;
	baseRevision: string;
	draftRevision: number;
	requestId: number;
}
export interface Preview extends Binding {
	nodes: readonly PolicyNode[];
	diff: readonly string[];
}
export interface Approval extends Binding {
	receipt: string;
}
export interface Ack extends Binding {
	status: "acked" | "published_not_adopted" | "publication_uncertain";
	snapshot: Snapshot;
	receipt: string;
	/** Required for applied: actual runtime adoption generation. */
	adoptionGeneration?: string;
}
/** Host integration must obtain receipts from protected operator authority, never from model text. */
export interface TrustedAuthority {
	preview(binding: Binding, edits: readonly Edit[]): Promise<Preview>;
	apply(binding: Binding, approval: Approval): Promise<Ack>;
}
export interface EditorState {
	current: Snapshot;
	draftRevision: number;
	edits: readonly Edit[];
	preview?: Preview;
	approval?: Approval;
	applied?: Snapshot;
	publishedNotAdopted?: Ack;
	publicationUncertain?: Ack;
	pending?: "preview" | "apply";
	error?: string;
}
export function sameBinding(a: Binding, b: Binding): boolean {
	return (
		a.identity === b.identity &&
		a.baseRevision === b.baseRevision &&
		a.draftRevision === b.draftRevision &&
		a.requestId === b.requestId
	);
}
/** Revisioned draft owner; not a policy evaluator. Inputs are authoritative projections. */
export function createPolicyEditor(
	initial: Snapshot,
	authority?: TrustedAuthority,
) {
	let state: EditorState = { current: initial, draftRevision: 0, edits: [] };
	let sequence = 0;
	let active: Binding | undefined;
	const listeners = new Set<() => void>();
	const publish = (next: EditorState) => {
		state = next;
		for (const fn of listeners) fn();
	};
	const binding = (): Binding => ({
		identity: state.current.identity,
		baseRevision: state.current.revision,
		draftRevision: state.draftRevision,
		requestId: ++sequence,
	});
	const matches = (b: Binding) =>
		active !== undefined &&
		sameBinding(active, b) &&
		b.identity === state.current.identity &&
		b.baseRevision === state.current.revision &&
		b.draftRevision === state.draftRevision;
	return {
		subscribe: (fn: () => void) => {
			listeners.add(fn);
			return () => {
				listeners.delete(fn);
			};
		},
		getSnapshot: () => state,
		edit(edit: Edit) {
			if (state.publishedNotAdopted || state.publicationUncertain) return false;
			const node = state.current.nodes.find((n) => n.id === edit.nodeId);
			if (
				!node ||
				node.locked ||
				node.lockReason ||
				!node.allowedChoices.includes(edit.mode) ||
				(state.current.projection === "containment_only" && edit.mode === "ask")
			)
				return false;
			active = undefined;
			publish({
				current: state.current,
				applied: state.applied,
				draftRevision: state.draftRevision + 1,
				edits: [
					...state.edits.filter((e) => e.nodeId !== edit.nodeId),
					{ ...edit },
				],
			});
			return true;
		},
		replace(snapshot: Snapshot) {
			active = undefined;
			publish({
				current: snapshot,
				draftRevision: state.draftRevision + 1,
				edits: [],
			});
		},
		async preview() {
			if (
				!authority ||
				!state.edits.length ||
				state.publishedNotAdopted ||
				state.publicationUncertain
			)
				return;
			const b = binding();
			active = b;
			publish({
				...state,
				preview: undefined,
				approval: undefined,
				pending: "preview",
				error: undefined,
			});
			try {
				const result = await authority.preview(b, state.edits);
				if (!matches(b)) return;
				if (!sameBinding(b, result))
					throw new Error("Preview binding mismatch");
				publish({ ...state, preview: result, pending: undefined });
			} catch (error) {
				if (matches(b))
					publish({ ...state, pending: undefined, error: String(error) });
			}
		},
		acceptApproval(approval: Approval) {
			if (
				!state.preview ||
				!sameBinding(state.preview, approval) ||
				!approval.receipt ||
				!matches(approval)
			)
				return false;
			publish({ ...state, approval });
			return true;
		},
		canApply: () =>
			Boolean(
				authority &&
					state.current.projection === "containment_and_guard" &&
					state.preview &&
					state.approval &&
					!state.pending &&
					sameBinding(state.preview, state.approval),
			),
		async apply() {
			if (
				!authority ||
				state.current.projection !== "containment_and_guard" ||
				!state.preview ||
				!state.approval ||
				state.pending ||
				!sameBinding(state.preview, state.approval)
			)
				return;
			const b = state.preview;
			const approval = state.approval;
			publish({ ...state, pending: "apply", error: undefined });
			try {
				const ack = await authority.apply(b, approval);
				if (!matches(b)) return;
				if (ack.status === "publication_uncertain" && sameBinding(b, ack)) {
					active = undefined;
					publish({
						...state,
						pending: undefined,
						approval: undefined,
						publicationUncertain: ack,
					});
					return;
				}
				if (
					!sameBinding(b, ack) ||
					!ack.receipt ||
					ack.snapshot.identity !== b.identity ||
					ack.snapshot.revision === b.baseRevision
				)
					throw new Error("Invalid apply acknowledgement");
				if (ack.status === "published_not_adopted") {
					active = undefined;
					publish({
						...state,
						pending: undefined,
						approval: undefined,
						publishedNotAdopted: ack,
					});
					return;
				}
				if (ack.status !== "acked" || !ack.adoptionGeneration)
					throw new Error("Runtime adoption acknowledgement missing");
				active = undefined;
				publish({
					current: ack.snapshot,
					applied: ack.snapshot,
					edits: [],
					draftRevision: state.draftRevision + 1,
				});
			} catch (error) {
				if (matches(b))
					publish({
						...state,
						pending: undefined,
						approval: undefined,
						error: String(error),
					});
			}
		},
		hasAuthority: Boolean(authority),
	};
}
export type PolicyEditor = ReturnType<typeof createPolicyEditor>;
