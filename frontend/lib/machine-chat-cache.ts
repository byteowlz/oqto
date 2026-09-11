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

/**
 * The machine owns the conversation; this is only a cache in front of it, so it
 * is deliberately small. Only conversations actually opened are stored, only
 * their newest page, and only until the budget is needed by a newer read.
 */
export const CACHE_LIMITS = {
	/** A single conversation larger than this is left to the machine. */
	maxEntryBytes: 4 * 1024 * 1024,
	maxTotalBytes: 64 * 1024 * 1024,
	maxEntries: 50,
};

export type CachedChat<TMessage> = {
	key: string;
	shape: number;
	cachedAt: number;
	/** Last time this conversation was read, for eviction order. */
	lastReadAt: number;
	/** Approximate stored size, kept so eviction needs no second pass. */
	bytes: number;
	messages: TMessage[];
	hasMore: boolean;
	nextBefore?: string | null;
};

export type EvictionCandidate = {
	key: string;
	bytes: number;
	lastReadAt: number;
};

/**
 * Keys to drop so the cache stays inside its budget.
 *
 * Least recently read goes first: the conversation you are working in should
 * survive opening a run of older ones.
 */
export function selectEvictions(
	entries: EvictionCandidate[],
	limits: typeof CACHE_LIMITS = CACHE_LIMITS,
): string[] {
	const ordered = [...entries].sort((a, b) => a.lastReadAt - b.lastReadAt);
	let total = ordered.reduce((sum, entry) => sum + entry.bytes, 0);
	let count = ordered.length;
	const evict: string[] = [];

	for (const entry of ordered) {
		if (total <= limits.maxTotalBytes && count <= limits.maxEntries) break;
		evict.push(entry.key);
		total -= entry.bytes;
		count -= 1;
	}
	return evict;
}

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
	// Reading keeps a conversation alive against eviction.
	void withStore("readwrite", (store) =>
		store.put({ ...entry, lastReadAt: Date.now() }),
	);
	return entry;
}

export async function writeCachedChat<TMessage>(
	key: string,
	messages: TMessage[],
	hasMore: boolean,
	nextBefore?: string | null,
): Promise<void> {
	const now = Date.now();
	const bytes = approximateBytes(messages);
	// Caching a huge conversation would evict many useful ones to hold one that
	// is cheap to re-read a page at a time.
	if (bytes > CACHE_LIMITS.maxEntryBytes) return;

	const entry: CachedChat<TMessage> = {
		key,
		shape: CACHE_SHAPE_VERSION,
		cachedAt: now,
		lastReadAt: now,
		bytes,
		messages,
		hasMore,
		nextBefore: nextBefore ?? null,
	};
	await withStore("readwrite", (store) => store.put(entry));
	await enforceBudget();
}

function approximateBytes(value: unknown): number {
	try {
		return JSON.stringify(value)?.length ?? 0;
	} catch {
		return Number.POSITIVE_INFINITY;
	}
}

async function enforceBudget(): Promise<void> {
	const all = await withStore<CachedChat<unknown>[]>("readonly", (store) =>
		store.getAll(),
	);
	if (!all) return;
	const evict = selectEvictions(
		all.map((entry) => ({
			key: entry.key,
			bytes: entry.bytes ?? 0,
			lastReadAt: entry.lastReadAt ?? entry.cachedAt ?? 0,
		})),
	);
	for (const key of evict) {
		await withStore("readwrite", (store) => store.delete(key));
	}
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
