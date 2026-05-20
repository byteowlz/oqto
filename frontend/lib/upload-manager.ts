import { uploadFileMux } from "@/lib/mux-files";

export type UploadStatus =
	| "queued"
	| "uploading"
	| "done"
	| "failed"
	| "cancelled";

export type UploadJob = {
	id: string;
	workspacePath: string;
	destPath: string;
	fileName: string;
	size: number;
	loaded: number;
	status: UploadStatus;
	error?: string;
	createdAt: number;
	updatedAt: number;
};

type Listener = () => void;

const DB_NAME = "oqto_uploads";
const STORE = "jobs";
const CONCURRENCY = 4;

class UploadManager {
	private jobs = new Map<string, UploadJob>();
	private listeners = new Set<Listener>();
	private queue: Array<{ id: string; file: File }> = [];
	private inFlight = 0;
	private abortControllers = new Map<string, AbortController>();
	private fileBlobs = new Map<string, File>();
	private hydrated = false;
	private snapshot: UploadJob[] = [];
	private snapshotDirty = true;

	constructor() {
		void this.hydrate();
	}

	subscribe(listener: Listener): () => void {
		this.listeners.add(listener);
		return () => this.listeners.delete(listener);
	}

	getJobs(): UploadJob[] {
		if (this.snapshotDirty) {
			this.snapshot = Array.from(this.jobs.values()).sort(
				(a, b) => b.createdAt - a.createdAt,
			);
			this.snapshotDirty = false;
		}
		return this.snapshot;
	}

	enqueue(workspacePath: string, entries: Array<{ destPath: string; file: File }>) {
		for (const entry of entries) {
			const id = crypto.randomUUID();
			const now = Date.now();
			const job: UploadJob = {
				id,
				workspacePath,
				destPath: entry.destPath,
				fileName: entry.file.name,
				size: entry.file.size,
				loaded: 0,
				status: "queued",
				createdAt: now,
				updatedAt: now,
			};
			this.jobs.set(id, job);
			this.fileBlobs.set(id, entry.file);
			this.queue.push({ id, file: entry.file });
			void this.saveJob(job);
		}
		this.emit();
		void this.pump();
	}

	cancel(id: string) {
		const job = this.jobs.get(id);
		if (!job) return;
		if (job.status === "queued") {
			this.queue = this.queue.filter((q) => q.id !== id);
			job.status = "cancelled";
			job.updatedAt = Date.now();
			void this.saveJob(job);
			this.emit();
			return;
		}
		if (job.status === "uploading") {
			this.abortControllers.get(id)?.abort();
		}
	}

	retry(id: string) {
		const job = this.jobs.get(id);
		if (!job) return;
		if (job.status !== "failed" && job.status !== "cancelled") return;
		job.status = "queued";
		job.error = undefined;
		job.loaded = 0;
		job.updatedAt = Date.now();
		const file = this.fileBlobs.get(id);
		if (!file) {
			job.status = "failed";
			job.error = "Cannot retry after reload; please select file again";
			void this.saveJob(job);
			this.emit();
			return;
		}
		this.queue.push({ id, file });
		void this.saveJob(job);
		this.emit();
	}

	private async pump() {
		while (this.inFlight < CONCURRENCY && this.queue.length > 0) {
			const next = this.queue.shift();
			if (!next) return;
			const job = this.jobs.get(next.id);
			if (!job) continue;
			this.inFlight += 1;
			job.status = "uploading";
			job.updatedAt = Date.now();
			this.emit();
			void this.saveJob(job);

			const abortController = new AbortController();
			this.abortControllers.set(next.id, abortController);

			void uploadFileMux(
				job.workspacePath,
				job.destPath,
				next.file,
				(loaded, total) => {
					const current = this.jobs.get(next.id);
					if (!current) return;
					current.loaded = loaded;
					if (current.size === 0 && total > 0) current.size = total;
					current.updatedAt = Date.now();
					this.emit();
					void this.saveJob(current);
				},
				abortController.signal,
			)
				.then(() => {
					const current = this.jobs.get(next.id);
					if (!current) return;
					current.status = "done";
					current.loaded = current.size;
					current.updatedAt = Date.now();
					this.emit();
					void this.saveJob(current);
				})
				.catch((error: unknown) => {
					const current = this.jobs.get(next.id);
					if (!current) return;
					const message = error instanceof Error ? error.message : "Upload failed";
					if (message === "Upload cancelled") {
						current.status = "cancelled";
						current.error = undefined;
					} else {
						current.status = "failed";
						current.error = message;
					}
					current.updatedAt = Date.now();
					this.emit();
					void this.saveJob(current);
				})
				.finally(() => {
					this.abortControllers.delete(next.id);
					this.inFlight -= 1;
					void this.pump();
				});
		}
	}

	private emit() {
		this.snapshotDirty = true;
		for (const listener of this.listeners) listener();
	}

	private async hydrate() {
		if (this.hydrated) return;
		this.hydrated = true;
		const db = await openDb();
		if (!db) return;
		const tx = db.transaction(STORE, "readonly");
		const store = tx.objectStore(STORE);
		const req = store.getAll();
		const jobs: UploadJob[] = await promisify(req, []);
		for (const job of jobs) {
			if (job.status === "uploading" || job.status === "queued") {
				job.status = "failed";
				job.error = "Interrupted by reload; please retry";
			}
			this.jobs.set(job.id, job);
		}
		this.emit();
	}

	private async saveJob(job: UploadJob) {
		const db = await openDb();
		if (!db) return;
		const tx = db.transaction(STORE, "readwrite");
		tx.objectStore(STORE).put(job);
	}
}

let dbPromise: Promise<IDBDatabase | null> | null = null;

async function openDb(): Promise<IDBDatabase | null> {
	if (typeof indexedDB === "undefined") return null;
	if (dbPromise) return dbPromise;
	dbPromise = new Promise((resolve) => {
		const req = indexedDB.open(DB_NAME, 1);
		req.onupgradeneeded = () => {
			const db = req.result;
			if (!db.objectStoreNames.contains(STORE)) {
				db.createObjectStore(STORE, { keyPath: "id" });
			}
		};
		req.onsuccess = () => resolve(req.result);
		req.onerror = () => resolve(null);
	});
	return dbPromise;
}

function promisify<T>(request: IDBRequest<T>, fallback: T): Promise<T> {
	return new Promise((resolve) => {
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => resolve(fallback);
	});
}

export const uploadManager = new UploadManager();
