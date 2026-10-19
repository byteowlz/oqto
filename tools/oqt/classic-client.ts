// Human/operator transport: only immutable source bytes cross the guard socket.
// Neither original paths nor source cwd become execution authority.
import {
	type FrozenClassicSource,
	MAX_CLASSIC_SOURCE_BYTES,
	readClassicSource,
} from "./classic-source.ts";
import { WireClient } from "./guard-wire.ts";

export interface ClassicImportReceipt {
	readonly sessionId: string;
	readonly sourceHash: string;
	readonly status: "imported" | "unchanged";
	readonly mode?: string;
	readonly predecessorSessionId?: string;
	readonly reason?: string;
}
export interface ClassicClientOptions {
	readonly leafId?: string;
	readonly signal?: AbortSignal;
	readonly expectedHash?: string;
}

/** Only quoting/escaping, never expansion, subprocesses or shell evaluation. */
export function parseClassicResumeArgs(raw: string): {
	path?: string;
	origin: string;
	leafId?: string;
} {
	const tokens: string[] = [];
	let token = "";
	let quote = "";
	let escaped = false;
	let active = false;
	for (const char of raw) {
		if (escaped) {
			token += char;
			escaped = false;
			active = true;
			continue;
		}
		if (char === "\\" && quote !== "'") {
			escaped = true;
			active = true;
			continue;
		}
		if (quote) {
			if (char === quote) quote = "";
			else token += char;
			active = true;
			continue;
		}
		if (char === "'" || char === '"') {
			quote = char;
			active = true;
			continue;
		}
		if (/\s/.test(char)) {
			if (active) {
				tokens.push(token);
				token = "";
				active = false;
			}
		} else {
			token += char;
			active = true;
		}
	}
	if (quote || escaped)
		throw new Error("Unterminated quote or escape in classic resume options");
	if (active) tokens.push(token);
	if (tokens.shift() !== "--classic")
		throw new Error("Expected --classic import options");
	const result: { path?: string; origin?: string; leafId?: string } = {};
	if (tokens[0] !== undefined && !tokens[0].startsWith("--")) {
		result.path = tokens.shift();
		if (result.path === "")
			throw new Error("Classic source path must not be empty");
	}
	const seen = new Set<string>();
	for (let index = 0; index < tokens.length; index++) {
		const flag = tokens[index];
		if (seen.has(flag)) throw new Error("Duplicate classic resume option");
		seen.add(flag);
		if (flag !== "--origin" && flag !== "--leaf")
			throw new Error("Unsupported classic resume option");
		const value = tokens[++index];
		if (!value || value.startsWith("--"))
			throw new Error("Classic resume option requires a value");
		if (flag === "--origin") result.origin = value;
		else result.leafId = value;
	}
	if (
		!result.origin ||
		!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(result.origin)
	)
		throw new Error("Explicit stable --origin NAME is required");
	return {
		...(result.path ? { path: result.path } : {}),
		origin: result.origin,
		...(result.leafId ? { leafId: result.leafId } : {}),
	};
}
// Catalog identity is opaque and distinct from the classic source UUID.
const CATALOG_ID = /^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/;
function object(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === "object" && !Array.isArray(value);
}

export async function uploadClassicSource(
	operatorPath: string,
	source: Pick<FrozenClassicSource, "bytes" | "sourceHash">,
	origin: string,
	options: ClassicClientOptions = {},
): Promise<ClassicImportReceipt> {
	if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(origin))
		throw new Error("An explicit stable source origin is required");
	if (
		source.bytes.length > MAX_CLASSIC_SOURCE_BYTES ||
		!/^[a-f0-9]{64}$/.test(source.sourceHash)
	)
		throw new Error("Invalid immutable classic source");
	const signal = options.signal
		? AbortSignal.any([options.signal, AbortSignal.timeout(120000)])
		: AbortSignal.timeout(120000);
	signal.throwIfAborted();
	const client = await WireClient.connect(operatorPath);
	try {
		const begin = await client.call(
			"import_classic_begin",
			{
				origin,
				sourceHash: source.sourceHash,
				byteLength: source.bytes.length,
				...(options.leafId === undefined ? {} : { leafId: options.leafId }),
			},
			signal,
		);
		if (
			!object(begin) ||
			typeof begin.uploadId !== "string" ||
			!/^[A-Za-z0-9_-]{1,128}$/.test(begin.uploadId)
		)
			throw new Error("Invalid classic upload admission");
		const uploadId = begin.uploadId;
		for (let offset = 0; offset < source.bytes.length; ) {
			const part = source.bytes.subarray(offset, offset + 16 * 1024);
			const ack = await client.call(
				"import_classic_chunk",
				{ uploadId, offset, base64: part.toString("base64") },
				signal,
			);
			offset += part.length;
			if (!object(ack) || ack.offset !== offset)
				throw new Error("Classic upload acknowledgment mismatch");
		}
		const result = await client.call(
			"import_classic_commit",
			{ uploadId },
			signal,
		);
		if (
			!object(result) ||
			typeof result.sessionId !== "string" ||
			!CATALOG_ID.test(result.sessionId) ||
			result.sessionId.startsWith("pending-") ||
			result.sourceHash !== source.sourceHash ||
			(result.status !== "imported" && result.status !== "unchanged")
		)
			throw new Error(
				"Invalid classic publication receipt; inspect before retrying",
			);
		return result as unknown as ClassicImportReceipt;
	} finally {
		client.close();
	}
}

export async function importClassicSource(
	operatorPath: string,
	path: string,
	origin: string,
	options: ClassicClientOptions = {},
): Promise<ClassicImportReceipt> {
	const source = await readClassicSource(path, origin);
	if (
		options.expectedHash !== undefined &&
		options.expectedHash !== source.sourceHash
	)
		throw new Error(
			"Selected classic source changed since discovery; inspect and select it again",
		);
	return uploadClassicSource(operatorPath, source, origin, options);
}
