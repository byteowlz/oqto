import {
	type MoonshineHost,
	MoonshineHostPool,
} from "@/lib/voice/moonshine-host-pool";
import { describe, expect, it, vi } from "vitest";

function fakeHost() {
	return {
		onProgress: undefined as MoonshineHost["onProgress"],
		loadTranscriber: vi.fn(async () => undefined),
		createStream: vi.fn(
			async (_transcriber: string, _stream: string, _options?: unknown) =>
				undefined,
		),
		setListener: vi.fn(),
		start: vi.fn(async () => undefined),
		addAudio: vi.fn(),
		queuedSeconds: vi.fn(() => 0),
		stop: vi.fn(async () => undefined),
		closeStream: vi.fn(async (_stream: string) => undefined),
		close: vi.fn(),
	};
}
type FakeHost = ReturnType<typeof fakeHost>;

const en = {
	transcriberId: "ears",
	modelArch: 2,
	source: { kind: "catalog", language: "en", includeSpelling: false },
} as const;
const de = { ...en, source: { ...en.source, language: "de" } } as const;

function pool() {
	const hosts: FakeHost[] = [];
	const spawn = vi.fn(async () => {
		const host = fakeHost();
		hosts.push(host);
		return host as unknown as MoonshineHost;
	});
	return { pool: new MoonshineHostPool(spawn), spawn, hosts };
}

async function dictate(
	host: MoonshineHost,
	config: typeof en | typeof de = en,
) {
	await host.loadTranscriber(config);
	await host.createStream("ears", "audio", { updateInterval: 0.3 });
	await host.start("audio");
	await host.stop("audio");
	host.close();
}

describe("warm local speech model", () => {
	it("reuses one worker and one loaded model across dictations", async () => {
		const { pool: p, spawn, hosts } = pool();
		await dictate(await p.loader("/speech/"));
		await dictate(await p.loader("/speech/"));
		expect(spawn).toHaveBeenCalledOnce();
		const host = hosts[0] as FakeHost;
		expect(host.loadTranscriber).toHaveBeenCalledOnce();
		const streams = host.createStream.mock.calls.map((call) => call[1]);
		expect(new Set(streams).size).toBe(2);
		expect(host.closeStream.mock.calls.map((call) => call[0])).toEqual(streams);
		expect(host.close).not.toHaveBeenCalled();
	});

	it("replaces the worker when the language changes, keeping one model resident", async () => {
		const { pool: p, spawn, hosts } = pool();
		await dictate(await p.loader("/speech/"), en);
		await dictate(await p.loader("/speech/"), de);
		expect(spawn).toHaveBeenCalledTimes(2);
		expect(hosts[0]?.close).toHaveBeenCalledOnce();
		expect(hosts[1]?.loadTranscriber).toHaveBeenCalledWith(de);
		expect(hosts[1]?.createStream).toHaveBeenCalledOnce();
	});

	it("evicts a worker that failed so the next dictation starts clean", async () => {
		const { pool: p, spawn, hosts } = pool();
		const first = await p.loader("/speech/");
		await first.loadTranscriber(en);
		await first.createStream("ears", "audio", { updateInterval: 0.3 });
		hosts[0]?.start.mockRejectedValueOnce(new Error("worker crashed"));
		await expect(first.start("audio")).rejects.toThrow("worker crashed");
		expect(hosts[0]?.close).toHaveBeenCalledOnce();
		await dictate(await p.loader("/speech/"));
		expect(spawn).toHaveBeenCalledTimes(2);
		expect(hosts[1]?.loadTranscriber).toHaveBeenCalledOnce();
	});

	it("does not touch the worker when cancelled before a stream existed", async () => {
		const { pool: p, hosts } = pool();
		const lease = await p.loader("/speech/");
		lease.close();
		expect(hosts[0]?.closeStream).not.toHaveBeenCalled();
		expect(hosts[0]?.close).not.toHaveBeenCalled();
		await dictate(await p.loader("/speech/"));
		expect(hosts).toHaveLength(1);
	});

	it("shares a model load that is still in flight", async () => {
		const { pool: p, hosts } = pool();
		const [a, b] = [await p.loader("/speech/"), await p.loader("/speech/")];
		await Promise.all([a.loadTranscriber(en), b.loadTranscriber(en)]);
		expect(hosts[0]?.loadTranscriber).toHaveBeenCalledOnce();
	});
});
