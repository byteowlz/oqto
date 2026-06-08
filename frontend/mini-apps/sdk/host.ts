/**
 * The OqtoHost contract: the only surface through which a mini-app reaches the
 * outside world. In the standalone workbench this is backed by createMockHost();
 * in the integration phase it will be backed by the live oqto services. The
 * shape is intentionally promise-based and serializable (refs, Blobs, plain
 * values -- never raw handles or DOM nodes) so the exact same interface can be
 * implemented over a postMessage/iframe bridge later with no app-side changes.
 */

import type { Base24Scheme, ThemeMode } from "@/mini-apps/theming";

export interface OqtoFileRef {
	id: string;
	name: string;
	mime: string;
	size: number;
}

export interface OqtoFilePickOptions {
	/** Accept attribute / mime filter, e.g. "image/*". */
	accept?: string;
}

export interface OqtoFilesCapability {
	/** Prompt the user to pick a file. Resolves null if cancelled. */
	pick(opts?: OqtoFilePickOptions): Promise<OqtoFileRef | null>;
	/** Prompt the user to pick one or more files. Resolves [] if cancelled. */
	pickMultiple(opts?: OqtoFilePickOptions): Promise<OqtoFileRef[]>;
	/** Read the bytes for a previously picked/produced ref. */
	read(ref: OqtoFileRef): Promise<Blob>;
	/** Persist bytes under a name. Standalone implementation triggers download. */
	write(name: string, data: Blob): Promise<OqtoFileRef>;
}

export interface OqtoKvCapability {
	get<T>(key: string): Promise<T | null>;
	set<T>(key: string, value: T): Promise<void>;
	remove(key: string): Promise<void>;
}

export type OqtoNotificationLevel = "info" | "success" | "error";

export interface OqtoNotificationsCapability {
	notify(message: string, level?: OqtoNotificationLevel): void;
}

export interface OqtoThemeCapability {
	getMode(): ThemeMode;
	getScheme(): Base24Scheme;
	setMode(mode: ThemeMode): void;
}

export interface OqtoUser {
	id: string;
	name: string;
	email?: string;
}

export interface OqtoUserCapability {
	getUser(): OqtoUser | null;
}

export interface OqtoHost {
	readonly version: string;
	readonly files: OqtoFilesCapability;
	readonly kv: OqtoKvCapability;
	readonly notifications: OqtoNotificationsCapability;
	readonly theme: OqtoThemeCapability;
	readonly user: OqtoUserCapability;
}

/** The capability keys an app may declare it needs. */
export type OqtoCapabilityKey = Exclude<keyof OqtoHost, "version">;
