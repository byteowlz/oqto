import {
	PHONON_FILES,
	PHONON_REVISION,
	PHONON_WASM,
	loadPhononModel,
	loadPhononWasm,
} from "@/lib/voice/phonon-model";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const NativeURL = URL;
const stored = new Map<string, Response>();
const cache = {
	match: vi.fn(async (url: string) => stored.get(url)?.clone()),
	put: vi.fn(async (url: string, response: Response) => {
		stored.set(url, response);
	}),
	delete: vi.fn(async (url: string) => stored.delete(url)),
};
const created = vi.fn(() => `blob:verified-${Math.random()}`);
const revoked = vi.fn();
const digest = vi.fn(async (_algorithm: string, bytes: Uint8Array) => {
	const spec = [
		...Object.values(PHONON_FILES),
		...Object.values(PHONON_WASM),
	].find((file) => file.bytes === bytes.length);
	if (!spec) throw new Error("Unexpected digest input");
	return Uint8Array.from(spec.sha256.match(/../g) ?? [], (hex) =>
		Number.parseInt(hex, 16),
	).buffer;
});
function response(size: number) {
	let remaining = size;
	return new Response(
		new ReadableStream({
			pull(controller) {
				if (remaining === 0) {
					controller.close();
					return;
				}
				const count = Math.min(remaining, 1 << 20);
				remaining -= count;
				controller.enqueue(new Uint8Array(count));
			},
		}),
	);
}
beforeEach(() => {
	stored.clear();
	vi.clearAllMocks();
	vi.stubGlobal(
		"URL",
		class extends NativeURL {
			static createObjectURL = created;
			static revokeObjectURL = revoked;
		},
	);
	vi.stubGlobal("crypto", { subtle: { digest } });
	vi.stubGlobal("caches", { open: vi.fn(async () => cache) });
	vi.mocked(fetch).mockImplementation(async (input, init) => {
		const runtime = Object.values(PHONON_WASM).find((file) =>
			String(input).endsWith(file.file),
		);
		if (runtime) return response(runtime.bytes);
		expect(String(input)).toContain(`/resolve/${PHONON_REVISION}/`);
		expect(init).toMatchObject({
			credentials: "omit",
			referrerPolicy: "no-referrer",
		});
		const file = String(input).split("/").at(-1) as keyof typeof PHONON_FILES;
		return response(PHONON_FILES[file].bytes);
	});
});
afterEach(() => {
	vi.unstubAllGlobals();
});
it("verifies/caches both WASM variants as BufferSources and avoids HTTP-cache dependence on a warm retry", async () => {
	const base = new URL("/speech/phonon-0.4.0/", location.href);
	const first = await loadPhononWasm(base, new AbortController().signal);
	expect(first.wasmUrl).toBeInstanceOf(ArrayBuffer);
	expect(first.wasmUrl.byteLength).toBe(4387046);
	expect(first.simdWasmUrl.byteLength).toBe(4347985);
	expect(caches.open).toHaveBeenCalledWith("oqto-phonon-runtime-v1");
	expect(fetch).toHaveBeenCalledTimes(2);
	vi.mocked(fetch).mockRejectedValue(new Error("offline"));
	const warm = await loadPhononWasm(base, new AbortController().signal);
	expect(warm.wasmUrl.byteLength).toBe(first.wasmUrl.byteLength);
	expect(fetch).toHaveBeenCalledTimes(2);
	expect(digest).toHaveBeenCalledTimes(4);
	expect(created).not.toHaveBeenCalled();
});
it("hash-verifies every pinned asset before Blob creation, aggregates bytes, and reuses page cache offline", async () => {
	const progress = vi.fn();
	const assets = await loadPhononModel(new AbortController().signal, progress);
	expect(fetch).toHaveBeenCalledTimes(4);
	expect(digest).toHaveBeenCalledTimes(4);
	expect(cache.put).toHaveBeenCalledTimes(4);
	expect(progress).toHaveBeenLastCalledWith({
		loaded: 65049306,
		total: 65049306,
	});
	expect(Object.keys(assets.files)).toHaveLength(4);
	assets.release();
	expect(revoked).toHaveBeenCalledTimes(4);
	vi.mocked(fetch).mockRejectedValue(new Error("offline"));
	const warm = await loadPhononModel(new AbortController().signal, vi.fn());
	expect(fetch).toHaveBeenCalledTimes(4);
	expect(digest).toHaveBeenCalledTimes(8);
	warm.release();
});
it("rejects corrupt bytes before worker use and removes their cache entry", async () => {
	const url = `https://huggingface.co/gradium/phonon-31m-en/resolve/${PHONON_REVISION}/model.q8.gguf`;
	stored.set(url, response(PHONON_FILES["model.q8.gguf"].bytes));
	digest.mockResolvedValueOnce(new Uint8Array(32).buffer);
	await expect(
		loadPhononModel(new AbortController().signal, vi.fn()),
	).rejects.toThrow("checksum mismatch");
	expect(created).not.toHaveBeenCalled();
	expect(cache.put).not.toHaveBeenCalled();
	expect(cache.delete).toHaveBeenCalledWith(url);
	expect(stored.has(url)).toBe(false);
	const retry = await loadPhononModel(new AbortController().signal, vi.fn());
	expect(fetch).toHaveBeenCalledTimes(4);
	retry.release();
});
it.each([1, PHONON_FILES["model.q8.gguf"].bytes + 1])(
	"evicts a corrupt cached asset of %s bytes so explicit Retry downloads again",
	async (size) => {
		const url = `https://huggingface.co/gradium/phonon-31m-en/resolve/${PHONON_REVISION}/model.q8.gguf`;
		stored.set(url, response(size));
		await expect(
			loadPhononModel(new AbortController().signal, vi.fn()),
		).rejects.toThrow(/size/);
		expect(cache.delete).toHaveBeenCalledWith(url);
		expect(fetch).not.toHaveBeenCalled();
		const retry = await loadPhononModel(new AbortController().signal, vi.fn());
		expect(fetch).toHaveBeenCalledTimes(4);
		retry.release();
	},
);
it("does not evict a cached asset on ordinary user cancellation", async () => {
	const url = `https://huggingface.co/gradium/phonon-31m-en/resolve/${PHONON_REVISION}/model.q8.gguf`;
	stored.set(url, response(PHONON_FILES["model.q8.gguf"].bytes));
	const abort = new AbortController();
	await expect(
		loadPhononModel(abort.signal, () => abort.abort()),
	).rejects.toMatchObject({ name: "AbortError" });
	expect(cache.delete).not.toHaveBeenCalled();
	expect(stored.has(url)).toBe(true);
});
it("rejects truncated/oversized downloads, and aborts rather than keeping partial assets", async () => {
	vi.mocked(fetch).mockResolvedValueOnce(response(1));
	await expect(
		loadPhononModel(new AbortController().signal, vi.fn()),
	).rejects.toThrow("size mismatch");
	const abort = new AbortController();
	await expect(
		loadPhononModel(abort.signal, () => abort.abort()),
	).rejects.toMatchObject({ name: "AbortError" });
	expect(cache.put).not.toHaveBeenCalled();
	expect(created).not.toHaveBeenCalled();
});
