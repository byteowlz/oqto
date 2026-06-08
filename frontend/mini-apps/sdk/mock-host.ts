import {
	type Base24Scheme,
	type ThemeMode,
	defaultSchemeForMode,
} from "@/mini-apps/theming";
import { toast } from "sonner";
import type {
	OqtoFilePickOptions,
	OqtoFileRef,
	OqtoHost,
	OqtoNotificationLevel,
	OqtoUser,
} from "./host";

export interface MockHostOptions {
	user?: OqtoUser | null;
	kvNamespace?: string;
	getMode?: () => ThemeMode;
	getScheme?: () => Base24Scheme;
	setMode?: (mode: ThemeMode) => void;
}

let idCounter = 0;

function newId(): string {
	idCounter += 1;
	// crypto.randomUUID requires a secure context (https/localhost). On a plain
	// http LAN origin it is undefined, so fall back to a monotonic counter --
	// never a coarse timer, which can collide within a single batch pick.
	if (
		typeof crypto !== "undefined" &&
		typeof crypto.randomUUID === "function"
	) {
		return crypto.randomUUID();
	}
	return `id-${idCounter}-${Date.now()}`;
}

/** Open a transient file input and resolve with the chosen files (or []). */
function openFileDialog(
	accept: string | undefined,
	multiple: boolean,
): Promise<File[]> {
	return new Promise((resolve) => {
		const input = document.createElement("input");
		input.type = "file";
		if (accept) input.accept = accept;
		input.multiple = multiple;
		input.style.display = "none";

		let settled = false;
		const finish = (files: File[]) => {
			if (settled) return;
			settled = true;
			input.remove();
			resolve(files);
		};

		input.addEventListener("change", () => {
			finish(input.files ? Array.from(input.files) : []);
		});
		// Cancelling the OS dialog fires no change event on most browsers.
		input.addEventListener("cancel", () => finish([]));

		document.body.appendChild(input);
		input.click();
	});
}

/**
 * A fully local OqtoHost for standalone prototyping. No backend, no oqto shell.
 * - files.pick: a transient <input type="file">; bytes are held in memory.
 * - files.write: triggers a browser download.
 * - kv: namespaced localStorage.
 * - notifications: sonner toasts (the shell renders the Toaster).
 * - theme: delegates to the shell's getters/setters when provided.
 */
export function createMockHost(opts: MockHostOptions = {}): OqtoHost {
	const namespace = opts.kvNamespace ?? "default";
	const kvPrefix = `oqto-miniapp:${namespace}:`;
	const blobs = new Map<string, Blob>();

	let currentMode: ThemeMode = opts.getMode?.() ?? "dark";

	const fileToRef = (file: File): OqtoFileRef => {
		const ref: OqtoFileRef = {
			id: newId(),
			name: file.name,
			mime: file.type || "application/octet-stream",
			size: file.size,
		};
		blobs.set(ref.id, file);
		return ref;
	};

	return {
		version: "0.1.0-mock",

		files: {
			async pick(options?: OqtoFilePickOptions): Promise<OqtoFileRef | null> {
				const files = await openFileDialog(options?.accept, false);
				return files[0] ? fileToRef(files[0]) : null;
			},

			async pickMultiple(
				options?: OqtoFilePickOptions,
			): Promise<OqtoFileRef[]> {
				const files = await openFileDialog(options?.accept, true);
				return files.map(fileToRef);
			},

			read(ref: OqtoFileRef): Promise<Blob> {
				const blob = blobs.get(ref.id);
				if (!blob) {
					return Promise.reject(
						new Error(`No bytes held for file ref ${ref.id}`),
					);
				}
				return Promise.resolve(blob);
			},

			write(name: string, data: Blob): Promise<OqtoFileRef> {
				const url = URL.createObjectURL(data);
				const a = document.createElement("a");
				a.href = url;
				a.download = name;
				document.body.appendChild(a);
				a.click();
				a.remove();
				URL.revokeObjectURL(url);
				const ref: OqtoFileRef = {
					id: newId(),
					name,
					mime: data.type || "application/octet-stream",
					size: data.size,
				};
				blobs.set(ref.id, data);
				return Promise.resolve(ref);
			},
		},

		kv: {
			get<T>(key: string): Promise<T | null> {
				try {
					const raw = localStorage.getItem(kvPrefix + key);
					return Promise.resolve(raw ? (JSON.parse(raw) as T) : null);
				} catch {
					return Promise.resolve(null);
				}
			},
			set<T>(key: string, value: T): Promise<void> {
				try {
					localStorage.setItem(kvPrefix + key, JSON.stringify(value));
				} catch {
					// ignore quota / serialization errors in the mock
				}
				return Promise.resolve();
			},
			remove(key: string): Promise<void> {
				localStorage.removeItem(kvPrefix + key);
				return Promise.resolve();
			},
		},

		notifications: {
			notify(message: string, level: OqtoNotificationLevel = "info"): void {
				if (level === "error") toast.error(message);
				else if (level === "success") toast.success(message);
				else toast(message);
			},
		},

		theme: {
			getMode(): ThemeMode {
				currentMode = opts.getMode?.() ?? currentMode;
				return currentMode;
			},
			getScheme(): Base24Scheme {
				return opts.getScheme?.() ?? defaultSchemeForMode(currentMode);
			},
			setMode(mode: ThemeMode): void {
				currentMode = mode;
				opts.setMode?.(mode);
			},
		},

		user: {
			getUser(): OqtoUser | null {
				return opts.user ?? { id: "local", name: "Local User" };
			},
		},
	};
}
