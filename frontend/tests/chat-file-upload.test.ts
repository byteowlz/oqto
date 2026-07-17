import { beforeEach, describe, expect, it, vi } from "vitest";

import { FILE_UPLOAD_TIMEOUT_MS, uploadFileHttp } from "@/lib/mux-files";

class FakeXmlHttpRequest {
	static latest: FakeXmlHttpRequest | null = null;

	status = 0;
	responseText = "";
	statusText = "";
	timeout = 0;
	withCredentials = false;
	upload = { onprogress: null as ((event: ProgressEvent) => void) | null };
	onload: (() => void) | null = null;
	onerror: (() => void) | null = null;
	onabort: (() => void) | null = null;
	ontimeout: (() => void) | null = null;

	constructor() {
		FakeXmlHttpRequest.latest = this;
	}

	open = vi.fn();
	send = vi.fn();
	abort = vi.fn(() => this.onabort?.());
}

describe("chat file upload", () => {
	beforeEach(() => {
		FakeXmlHttpRequest.latest = null;
		vi.stubGlobal("XMLHttpRequest", FakeXmlHttpRequest);
	});

	it("rejects a stalled upload instead of spinning indefinitely", async () => {
		const upload = uploadFileHttp(
			"/tmp/workspace",
			"uploads/image.png",
			new File(["image"], "image.png", { type: "image/png" }),
		);
		const xhr = FakeXmlHttpRequest.latest;
		expect(xhr).not.toBeNull();
		expect(xhr?.timeout).toBe(FILE_UPLOAD_TIMEOUT_MS);

		xhr?.ontimeout?.();

		await expect(upload).rejects.toThrow("Upload timed out after 30 minutes");
	});

	it("resolves a successful upload", async () => {
		const upload = uploadFileHttp(
			"/tmp/workspace",
			"uploads/image.png",
			new File(["image"], "image.png", { type: "image/png" }),
		);
		const xhr = FakeXmlHttpRequest.latest;
		if (!xhr) throw new Error("XHR was not created");
		xhr.status = 201;
		xhr.onload?.();

		await expect(upload).resolves.toBeUndefined();
	});
});
