/**
 * Device-local layout document storage (ADR-0037: Account-namespaced,
 * device/Screen-Mode-local, disposable). The compositor kernel never touches
 * storage; this adapter moves opaque document strings in and out of the
 * browser, failing soft when storage is unavailable.
 */

export interface LayoutDocumentStore {
	read(key: string): string | null;
	write(key: string, layoutDocument: string): void;
}

export function layoutStorageKey(
	platformId: string,
	screenMode: string,
): string {
	return `oqto-ui:layout:${platformId}:${screenMode}`;
}

export const browserLayoutStorage: LayoutDocumentStore = {
	read(key) {
		try {
			return localStorage.getItem(key) ?? null;
		} catch {
			// Private mode or blocked storage: behave as an empty store.
			return null;
		}
	},
	write(key, layoutDocument) {
		try {
			localStorage.setItem(key, layoutDocument);
		} catch {
			// Quota or blocked storage: the in-memory snapshot stays authoritative.
		}
	},
};

export function memoryLayoutStorage(): LayoutDocumentStore {
	const documents = new Map<string, string>();
	return {
		read: (key) => documents.get(key) ?? null,
		write: (key, layoutDocument) => {
			documents.set(key, layoutDocument);
		},
	};
}
