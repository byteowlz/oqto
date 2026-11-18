import runtimeManifest from "../../scripts/phonon-runtime-manifest.json";

/** Public Apache-2.0 English checkpoint; not Pocket newtiny or German. */
export const PHONON_REVISION = "52433c46975bfa13144c56204c9f4ee1c5b148aa";
const BASE = `https://huggingface.co/gradium/phonon-31m-en/resolve/${PHONON_REVISION}/`;
export const PHONON_FILES = {
	"model.q8.gguf": {
		bytes: 64512352,
		sha256: "a7d9aad2cd6dc66473f24a08bc6d2939030ef4d751ef82349361fbef3272dbae",
	},
	"config.json": {
		bytes: 2851,
		sha256: "7062ea376e6b158543c8abec8e5af67df171c49ad70eb7e2a0fb25c16f265e06",
	},
	"tokenizer.json": {
		bytes: 277967,
		sha256: "691a00068fa7b989f2f3e5ae5daa2d1a5bb319f2d5360deafcfb19a8bb6e7936",
	},
	"default-voice.safetensors": {
		bytes: 256136,
		sha256: "f739e8c4c76ee7f92e7c94a82002748c8cd22e99582ac49f7b9b94cd63786e18",
	},
} as const;
export interface TTSDownloadProgress {
	loaded: number;
	total: number;
}
export interface PhononModelAssets {
	files: Record<keyof typeof PHONON_FILES, string>;
	release: () => void;
}
const TOTAL = Object.values(PHONON_FILES).reduce(
	(sum, file) => sum + file.bytes,
	0,
);

export const PHONON_WASM = {
	wasmUrl: {
		file: "wasm/phonon_tts_bg.wasm",
		bytes: 4387046,
		sha256: runtimeManifest.files["wasm/phonon_tts_bg.wasm"],
	},
	simdWasmUrl: {
		file: "wasm/phonon_tts_simd_bg.wasm",
		bytes: 4347985,
		sha256: runtimeManifest.files["wasm/phonon_tts_simd_bg.wasm"],
	},
} as const;

/** Supported by 0.4.0 worker moduleFor -> wasm-bindgen module_or_path BufferSource.
 * Cache both SIMD variants: upstream selects one without any code fork or Blob URL.
 */
export async function loadPhononWasm(base: URL, signal: AbortSignal) {
	if (base.origin !== location.origin)
		throw new Error("Phonon runtime must be same-origin");
	let cache: Cache | undefined;
	try {
		cache = await globalThis.caches?.open("oqto-phonon-runtime-v1");
	} catch {
		/* Verified network load remains available. */
	}
	const buffers = {} as Record<keyof typeof PHONON_WASM, ArrayBuffer>;
	for (const [key, spec] of Object.entries(PHONON_WASM)) {
		signal.throwIfAborted();
		const bytes = await verifiedAsset(
			new URL(spec.file, base).href,
			spec,
			signal,
			() => {},
			cache,
		);
		buffers[key as keyof typeof PHONON_WASM] = bytes.buffer;
	}
	return buffers;
}

class InvalidModelAsset extends Error {}

async function readBounded(
	response: Response,
	size: number,
	signal: AbortSignal,
	onBytes: (loaded: number) => void,
) {
	if (!response.ok)
		throw new InvalidModelAsset(`Speech model GET failed (${response.status})`);
	const bytes = new Uint8Array(size);
	const reader = response.body?.getReader();
	if (!reader)
		throw new InvalidModelAsset(
			"Streaming speech model downloads are unavailable",
		);
	let loaded = 0;
	try {
		for (;;) {
			signal.throwIfAborted();
			const chunk = await reader.read().catch((error: Error) => {
				if (error.name === "AbortError") throw error;
				throw new InvalidModelAsset("Speech model stream failed");
			});
			if (chunk.done) break;
			if (loaded + chunk.value.length > size)
				throw new InvalidModelAsset("Speech model exceeds pinned size");
			bytes.set(chunk.value, loaded);
			loaded += chunk.value.length;
			onBytes(loaded);
		}
	} finally {
		// Cancellation of a cached Response clone can wait forever for its sibling
		// tee branch. Release our reader without waiting on that unrelated consumer.
		void reader.cancel().catch(() => {});
		reader.releaseLock();
	}
	if (loaded !== size)
		throw new InvalidModelAsset("Speech model size mismatch");
	return bytes;
}

async function verifiedAsset(
	url: string,
	spec: { bytes: number; sha256: string },
	signal: AbortSignal,
	onBytes: (loaded: number) => void,
	cache?: Cache,
) {
	const hit = await cache?.match(url).catch(() => undefined);
	signal.throwIfAborted();
	try {
		const response =
			hit ??
			(await fetch(url, {
				signal,
				credentials: "omit",
				referrerPolicy: "no-referrer",
			}));
		const bytes = await readBounded(response, spec.bytes, signal, onBytes);
		const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
		const hash = Array.from(digest, (byte) =>
			byte.toString(16).padStart(2, "0"),
		).join("");
		signal.throwIfAborted();
		if (hash !== spec.sha256)
			throw new InvalidModelAsset(
				"Speech model checksum mismatch; retry to download again",
			);
		if (!hit) await cache?.put(url, new Response(bytes)).catch(() => {});
		signal.throwIfAborted();
		return bytes;
	} catch (error) {
		if (hit && error instanceof InvalidModelAsset && !signal.aborted)
			await cache?.delete(url).catch(() => false);
		throw error;
	}
}

/** Verify cached and fresh bytes before inference; worker receives only verified Blob URLs. */
export async function loadPhononModel(
	signal: AbortSignal,
	onProgress: (progress: TTSDownloadProgress) => void,
): Promise<PhononModelAssets> {
	const files = {} as PhononModelAssets["files"];
	const release = () => {
		for (const url of Object.values(files)) URL.revokeObjectURL(url);
	};
	let cache: Cache | undefined;
	try {
		// Same cache as upstream, but writes belong to the page (including WebKit).
		cache = await globalThis.caches?.open("phonon-tts-v1");
	} catch {
		// Private browsing/quota policy can disable caching; verified GET still works.
	}
	let completed = 0;
	try {
		for (const [name, spec] of Object.entries(PHONON_FILES)) {
			signal.throwIfAborted();
			const url = BASE + name;
			const bytes = await verifiedAsset(
				url,
				spec,
				signal,
				(loaded) => onProgress({ loaded: completed + loaded, total: TOTAL }),
				cache,
			);
			files[name as keyof typeof PHONON_FILES] = URL.createObjectURL(
				new Blob([bytes]),
			);
			completed += spec.bytes;
		}
		return { files, release };
	} catch (error) {
		release();
		throw error;
	}
}
