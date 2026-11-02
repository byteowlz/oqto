import { describe, expect, it, vi } from "vitest";
import { createFilesStore } from "../../src/oqto-ui/files/store";
import type { FileChange } from "../../src/oqto-ui/platform/files-contract";
import { createFlashHosts } from "../../src/oqto-ui/platform/flash-hosts";
describe("Native tool completion refresh", () => {
	it("requeries when a committed invalidation arrives during an older listing", async () => {
		let notify: (event: FileChange) => void = () => {};
		let finish: (entries: []) => void = () => {};
		const old = new Promise<[]>((resolve) => {
			finish = resolve;
		});
		const list = vi
			.fn()
			.mockImplementationOnce(() => old)
			.mockResolvedValue([
				{
					path: "/workspace/proof.txt",
					name: "proof.txt",
					kind: "file",
					size: 18,
					mtimeMs: 1234,
				},
			]);
		const host = createFlashHosts({
			list,
			read: async () => "",
			subscribe(listener) {
				notify = listener;
				return () => {};
			},
		});
		const store = createFilesStore(host.files, "/workspace");
		notify({ kind: "resync", path: "", directory: true });
		finish([]);
		await vi.waitFor(() =>
			expect(store.getSnapshot().listings[""]).toMatchObject({
				status: "ready",
				entries: [{ name: "proof.txt", size: 18 }],
			}),
		);
		expect(list).toHaveBeenCalledTimes(2);
		store.dispose();
	});
});
