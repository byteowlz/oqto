// Guard-side owner credential broker. The only component that may renew owner
// OAuth credentials; it hands the brain access tokens, never refresh secrets.
import { chmod, readFile, rename, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { Credential } from "@earendil-works/pi-ai";
import { readOwnedJson } from "./readonly-profile.ts";

/** Renew one provider's credential exactly as owning Pi would: under the owner
 * auth-file lock, merging only that entry. Injected so tests can prove policy
 * without a live OAuth endpoint. */
export type OwnerRefresh = (providerId: string) => Promise<void>;

export const REFRESH_WINDOW_MS = 10 * 60_000;

function strip(value: unknown): Credential | undefined {
	if (value === undefined) return undefined;
	if (typeof value !== "object" || value === null || !("type" in value))
		throw new Error("Invalid owner credential");
	if (value.type === "api_key") {
		if (!("key" in value) || typeof value.key !== "string")
			throw new Error("Only static API-key credentials are supported");
		if (value.key.startsWith("!"))
			throw new Error("Credential commands are not executed by oqt");
		return { type: "api_key", key: value.key };
	}
	if (value.type === "oauth") {
		if (
			!("access" in value) ||
			typeof value.access !== "string" ||
			!("expires" in value) ||
			typeof value.expires !== "number" ||
			!Number.isFinite(value.expires)
		)
			throw new Error("Invalid OAuth credential");
		// Account metadata stays; the refresh secret never leaves the guard.
		return {
			...value,
			type: "oauth",
			access: value.access,
			expires: value.expires,
			refresh: "",
		};
	}
	throw new Error("Unsupported credential type");
}

export class OwnerBroker {
	readonly owner: string;
	readonly #refresh: OwnerRefresh | undefined;
	readonly #inflight = new Map<string, Promise<void>>();
	constructor(owner: string, refresh?: OwnerRefresh) {
		this.owner = owner;
		this.#refresh = refresh;
	}
	get authPath() {
		return join(this.owner, "auth.json");
	}
	/** One private pre-refresh copy beside the original (preserve-first owner config). */
	async #backup() {
		const target = join(this.owner, "auth.json.oqt-backup");
		const pending = `${target}.tmp`;
		await writeFile(pending, await readFile(this.authPath), { mode: 0o600 });
		await chmod(pending, 0o600);
		await rename(pending, target);
	}
	async read(providerId: string): Promise<Credential | undefined> {
		let raw = (await readOwnedJson(this.authPath))[providerId];
		if (
			raw &&
			typeof raw === "object" &&
			"type" in raw &&
			raw.type === "oauth" &&
			"expires" in raw &&
			typeof raw.expires === "number" &&
			raw.expires - Date.now() < REFRESH_WINDOW_MS
		) {
			if (!this.#refresh)
				throw new Error(
					"Owner OAuth token expires soon and no refresh broker is configured",
				);
			let renewal = this.#inflight.get(providerId);
			if (!renewal) {
				renewal = (async () => {
					await this.#backup();
					await this.#refresh?.(providerId);
				})().finally(() => this.#inflight.delete(providerId));
				this.#inflight.set(providerId, renewal);
			}
			await renewal;
			raw = (await readOwnedJson(this.authPath))[providerId];
		}
		return strip(raw);
	}
	async list() {
		const data = await readOwnedJson(this.authPath);
		return Object.entries(data).flatMap(([providerId, value]) =>
			value &&
			typeof value === "object" &&
			"type" in value &&
			(value.type === "api_key" || value.type === "oauth")
				? [{ providerId, type: value.type }]
				: [],
		);
	}
}
