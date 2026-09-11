/**
 * Durable cache for a machine's chat messages.
 *
 * A machine's history lives on that machine, so every reopen otherwise costs a
 * round trip and shows nothing while the machine is unreachable. Pages are kept
 * in IndexedDB rather than localStorage because a conversation can be tens of
 * megabytes, far past the localStorage budget the live chat cache works within.
 *
 * Entries are scoped by account and machine so one account never reads another's
 * history, and the store is versioned so a shape change discards stale rows
 * instead of rendering them.
 */

const DB_NAME = "oqto-machine-chats";
const DB_VERSION = 1;
const STORE = "messages";
const CACHE_SHAPE_VERSION = 1;

/** Cached messages are evicted once they are older than this. */
const MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;

export type CachedChat<TMessage> = {
	key: string;
	shape: number;
	cachedAt: number;
	messages: TMessage[];
	hasMore: boolean;
	nextBefore?: string | null;
};

export function machineChatKey(
	accountId: string,
	machineId: string,
	sessionId: string,
): string {
	return `${accountId}\u0000${machineId}\u0000${sessionId}`;
}

function openDb(): Promise<IDBDatabase | null> {
	if (typeof indexedDB === "undefined") return Promise.resolve(null);
	return new Promise((resolve) => {
		let request: IDBOpenDBRequest;
		try {
			request = indexedDB.open(DB_NAME, DB_VERSION);
		} catch {
			resolve(null);
			return;
		}
		request.onupgradeneeded = () => {
			const db = request.result;
			if (!db.objectStoreNames.contains(STORE)) {
				db.createObjectStore(STORE, { keyPath: "key" });
			}
		};
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => resolve(null);
		request.onblocked = () => resolve(null);
	});
}

function withStore<T>(
	mode: IDBTransactionMode,
	run: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T | null> {
	return openDb().then((db) => {
		if (!db) return null;
		return new Promise<T | null>((resolve) => {
			let request: IDBRequest<T>;
			try {
				request = run(db.transaction(STORE, mode).objectStore(STORE));
			} catch {
				db.close();
				resolve(null);
				return;
			}
			request.onsuccess = () => {
				resolve(request.result ?? null);
				db.close();
			};
			request.onerror = () => {
				resolve(null);
				db.close();
			};
		});
	});
}

/** Read a cached conversation, ignoring stale or foreign-shaped rows. */
export async function readCachedChat<TMessage>(
	key: string,
): Promise<CachedChat<TMessage> | null> {
	const entry = await withStore<CachedChat<TMessage>>("readonly", (store) =>
		store.get(key),
	);
	if (!entry || entry.shape !== CACHE_SHAPE_VERSION) return null;
	if (Date.now() - entry.cachedAt > MAX_AGE_MS) return null;
	return entry;
}

export async function writeCachedChat<TMessage>(
	key: string,
	messages: TMessage[],
	hasMore: boolean,
	nextBefore?: string | null,
): Promise<void> {
	const entry: CachedChat<TMessage> = {
		key,
		shape: CACHE_SHAPE_VERSION,
		cachedAt: Date.now(),
		messages,
		hasMore,
		nextBefore: nextBefore ?? null,
	};
	await withStore("readwrite", (store) => store.put(entry));
}

/**
 * Drop every cached conversation.
 *
 * Signing out must not leave one account's machine history readable on the
 * device for the next person to sign in.
 */
export async function clearCachedChats(): Promise<void> {
	await withStore("readwrite", (store) => store.clear());
}
