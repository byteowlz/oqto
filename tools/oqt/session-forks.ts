// Worker-owned native Durable navigation; no filesystem, model or authority access.
import { type Context, defineService } from "@earendil-works/chord";
import {
	defineDoc,
	type ConversationId,
	type ConversationRecord,
	type Cursor,
	type EntryId,
	type Harness,
	type Page,
} from "@earendil-works/pi-durable";

export interface ForkEntry {
	id: EntryId;
	conversationId: ConversationId;
	kind: string;
}
export interface SessionForks {
	selected(context: Context): Promise<ConversationId>;
	conversations(limit: number, cursor: Cursor | undefined, context: Context): Promise<Page<ConversationRecord, Cursor>>;
	entries(conversationId: ConversationId, limit: number, cursor: Cursor | undefined, context: Context): Promise<Page<ForkEntry, Cursor>>;
	select(conversationId: ConversationId, context: Context): Promise<void>;
	fork(conversationId: ConversationId, at: EntryId, context: Context): Promise<ConversationId>;
}
export const SessionForks = defineService<SessionForks>("oqt.session-forks");

// Selection is Session state, not fork-inherited conversation state or authority.
export const SelectedConversationDoc = defineDoc<{ conversationId: number }>({
	kind: "oqt.selected-conversation", version: 1, scope: "session",
	initial: () => ({ conversationId: 1 }),
});

function pageLimit(limit: number) {
	if (!Number.isSafeInteger(limit) || limit < 1 || limit > 100)
		throw new Error("Page limit must be an integer from 1 through 100");
	return limit;
}
function stableId(id: number) {
	if (!Number.isSafeInteger(id) || id < 1) throw new Error("Invalid native ID");
}

/** applySelection must rebind the worker's watch/input target before resolving.
 * Startup must call selected() and bind that handle before admitting input.
 * A failed rebind leaves the durable selection available for reconnect/retry.
 */
export function createSessionForks(
	harness: Harness,
	applySelection: (id: ConversationId, context: Context) => Promise<void>,
): SessionForks {
	const conversation = async (id: ConversationId, context: Context) => {
		stableId(id);
		const found = await harness.conversation(id, context);
		if (!found) throw new Error(`Unknown conversation ${id}`);
		return found;
	};
	return {
		async selected(context) {
			const state = await harness.snapshot(SelectedConversationDoc, context);
			const id = (state?.conversationId ?? 1) as ConversationId;
			await conversation(id, context); // fail closed; never silently return to root
			return id;
		},
		async conversations(limit, cursor, context) {
			return harness.scanConversations({}, pageLimit(limit), cursor, context);
		},
		async entries(id, limit, cursor, context) {
			const target = await conversation(id, context);
			const page = await target.entries({ order: "descending" }, pageLimit(limit), cursor, context);
			return { ...page, items: page.items.map(({ id, conversationId, kind }) => ({ id, conversationId, kind })) };
		},
		async select(id, context) {
			await conversation(id, context);
			await harness.commit(async tx => {
				(await tx.doc(SelectedConversationDoc)).conversationId = id;
			}, context);
			await applySelection(id, context);
		},
		async fork(id, at, context) {
			stableId(at);
			const parent = await conversation(id, context);
			// Native fork validates visibility (including inherited entries), copies no
			// tasks, and respects persisted document fork policies. Authority and
			// operator receipt docs MUST use fork: initial at their definitions.
			const child = await parent.fork(at, {
				ownership: { kind: "ownerless" },
				init: async (tx, childId) => {
					(await tx.doc(SelectedConversationDoc)).conversationId = childId;
				},
			}, context);
			await applySelection(child.id, context);
			return child.id;
		},
	};
}
