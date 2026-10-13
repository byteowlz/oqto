// Owning-machine credential access. Never persist/copy credentials to a demo profile.
import { constants } from "node:fs";
import { open } from "node:fs/promises";
import { userInfo } from "node:os";
import { join } from "node:path";
import type {
	AuthOperationOptions,
	Credential,
	CredentialStore,
} from "@earendil-works/pi-ai";

export async function readOwnedJson(
	path: string,
	privateOnly = true,
): Promise<Record<string, unknown>> {
	const handle = await open(
		path,
		constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
	);
	try {
		const limit = 8 * 1024 * 1024;
		const info = await handle.stat();
		if (
			!info.isFile() ||
			info.uid !== userInfo().uid ||
			info.mode & (privateOnly ? 0o077 : 0o022) ||
			info.size > limit
		)
			throw new Error(
				"Pi credentials must be an owned private bounded regular file",
			);
		const buffer = Buffer.alloc(limit + 1);
		let length = 0;
		while (length < buffer.length) {
			const { bytesRead } = await handle.read(
				buffer,
				length,
				buffer.length - length,
				length,
			);
			if (!bytesRead) break;
			length += bytesRead;
		}
		if (length > limit) throw new Error("Pi credential file too large");
		const data: unknown = JSON.parse(
			buffer.subarray(0, length).toString("utf8"),
		);
		if (typeof data !== "object" || data === null || Array.isArray(data))
			throw new Error("Invalid Pi configuration object");
		return data as Record<string, unknown>;
	} finally {
		await handle.close();
	}
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
	if (value.type === "oauth") {
		if (
			!("access" in value) ||
			typeof value.access !== "string" ||
			!("expires" in value) ||
			typeof value.expires !== "number" ||
			!Number.isFinite(value.expires)
		)
			throw new Error("Invalid OAuth credential");
		// The request resolver needs access and provider account metadata, not the
		// refresh secret. modify() refuses before running its refresh callback.
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

export function readOnlyCredentials(agentDirectory: string): CredentialStore {
	const path = join(agentDirectory, "auth.json");
	return {
		async read(providerId: string, options?: AuthOperationOptions) {
			options?.signal?.throwIfAborted();
			const data = await readOwnedJson(path);
			options?.signal?.throwIfAborted();
			return credential(data[providerId]);
		},
		async list(options?: AuthOperationOptions) {
			options?.signal?.throwIfAborted();
			const data = await readOwnedJson(path);
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
