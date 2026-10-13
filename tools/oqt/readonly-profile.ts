// Owning-machine credential access. Never persist/copy credentials to a demo profile.
import { lstat, readFile } from "node:fs/promises";
import { userInfo } from "node:os";
import { join } from "node:path";
import type {
	AuthOperationOptions,
	Credential,
	CredentialStore,
} from "@earendil-works/pi-ai";

async function ownedJson(path: string): Promise<Record<string, unknown>> {
	const info = await lstat(path);
	if (
		!info.isFile() ||
		info.uid !== userInfo().uid ||
		info.size > 8 * 1024 * 1024
	)
		throw new Error("Pi configuration must be an owned bounded regular file");
	const data: unknown = JSON.parse(await readFile(path, "utf8"));
	if (typeof data !== "object" || data === null || Array.isArray(data))
		throw new Error("Invalid Pi configuration object");
	return data as Record<string, unknown>;
}
function credential(value: unknown): Credential | undefined {
	if (value === undefined) return undefined;
	if (typeof value !== "object" || value === null)
		throw new Error("Invalid credential");
	if (!("type" in value)) throw new Error("Credential type required");
	if (value.type === "api_key") {
		if (!("key" in value) || typeof value.key !== "string")
			throw new Error("This stage requires a static API-key credential");
		if (value.key.startsWith("!"))
			throw new Error(
				"Credential commands are not allowed in read-only model configuration",
			);
		return { type: "api_key", key: value.key };
	}
	if (value.type === "oauth")
		throw new Error(
			"OAuth needs the owning Pi credential broker; read-only snapshots must not rotate refresh tokens",
		);
	throw new Error("Unsupported credential type");
}

export function readOnlyCredentials(agentDirectory: string): CredentialStore {
	const path = join(agentDirectory, "auth.json");
	return {
		async read(providerId: string, options?: AuthOperationOptions) {
			options?.signal?.throwIfAborted();
			const data = await ownedJson(path);
			options?.signal?.throwIfAborted();
			return credential(data[providerId]);
		},
		async list(options?: AuthOperationOptions) {
			options?.signal?.throwIfAborted();
			const data = await ownedJson(path);
			return Object.entries(data).flatMap(([providerId, value]) => {
				if (typeof value !== "object" || value === null || !("type" in value))
					return [];
				return value.type === "api_key" || value.type === "oauth"
					? [{ providerId, type: value.type }]
					: [];
			});
		},
		async modify() {
			throw new Error(
				"Pi credentials are read-only; refresh/login must use the owning credential broker",
			);
		},
		async delete() {
			throw new Error(
				"Pi credentials are read-only; logout must use the owning credential broker",
			);
		},
	};
}
