import {
	resetPlaybackGateForTests,
	setAppPlayback,
} from "@/lib/voice/playback-gate";
import {
	type WakeCapture,
	type WakeDeps,
	type WakeManifest,
	WakeService,
	loadVerifiedRelease,
} from "@/lib/voice/wake-service";
import { beforeEach, describe, expect, it, vi } from "vitest";

const sha = async (bytes: Uint8Array) => {
	const hash = await crypto.subtle.digest(
		"SHA-256",
		bytes as unknown as ArrayBuffer,
	);
	return [...new Uint8Array(hash)]
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
};

const MEL = new TextEncoder().encode("mel-bytes");
const EMB = new TextEncoder().encode("embedding-bytes");
const HEAD = new TextEncoder().encode("head-bytes");

async function release(
	overrides: Partial<Record<"mel" | "hey_oqto", string>> = {},
) {
	const manifest: WakeManifest = {
		release: "fixture-r1",
		threshold: 0.5,
		classifiers: ["hey_oqto"],
		files: {
			mel: { sha256: overrides.mel ?? (await sha(MEL)) },
			embedding: { sha256: await sha(EMB) },
			hey_oqto: { sha256: overrides.hey_oqto ?? (await sha(HEAD)) },
		},
	};
	return manifest;
}

function deps(manifest: WakeManifest, options: { capture?: false } = {}) {
	const files: Record<string, Uint8Array> = {
		mel: MEL,
		embedding: EMB,
		hey_oqto: HEAD,
	};
	const engine = {
		push: vi.fn(
			async () =>
				[] as { scores: Record<string, number>; detections: string[] }[],
		),
		setPlayback: vi.fn(),
		reset: vi.fn(),
	};
	const capture: WakeCapture & {
		close: ReturnType<typeof vi.fn>;
		setEnabled: ReturnType<typeof vi.fn>;
	} = {
		close: vi.fn(async () => undefined),
		setEnabled: vi.fn(async () => undefined),
	};
	let onSamples: ((pcm: Float32Array) => void) | undefined;
	const d: WakeDeps = {
		loadManifest: vi.fn(async () => manifest),
		loadFile: vi.fn(
			async (url: string) =>
				files[decodeURIComponent(url.split("/").pop() ?? "")] as Uint8Array,
		),
		createEngine: vi.fn(async () => engine),
		openCapture:
			options.capture === false
				? undefined
				: vi.fn(async (_rate, push) => {
						onSamples = push;
						return capture;
					}),
		digest: sha,
	};
	return { d, engine, capture, feed: (pcm: Float32Array) => onSamples?.(pcm) };
}

describe("wake release loading", () => {
	it("verifies every file against the manifest before creating an engine", async () => {
		const manifest = await release();
		const { d } = deps(manifest);
		const loaded = await loadVerifiedRelease(
			{ manifestUrl: "/m", filesUrl: "/f" },
			d,
		);
		expect(loaded.manifest.release).toBe("fixture-r1");
		expect(Object.keys(loaded.models.classifiers)).toEqual(["hey_oqto"]);
	});

	it("refuses a release whose bytes do not match the manifest hash", async () => {
		const manifest = await release({ hey_oqto: "0".repeat(64) });
		const { d } = deps(manifest);
		await expect(
			loadVerifiedRelease({ manifestUrl: "/m", filesUrl: "/f" }, d),
		).rejects.toThrow(/does not match its manifest hash/);
	});

	it("refuses a manifest without phrases, a sane threshold or a listed file", async () => {
		const manifest = await release();
		for (const broken of [
			{ ...manifest, classifiers: [] },
			{ ...manifest, threshold: 1.4 },
			{ ...manifest, files: { ...manifest.files, hey_oqto: undefined } },
		]) {
			const { d } = deps(broken as WakeManifest);
			await expect(
				loadVerifiedRelease({ manifestUrl: "/m", filesUrl: "/f" }, d),
			).rejects.toThrow();
		}
	});
});

describe("wake service", () => {
	beforeEach(() => resetPlaybackGateForTests());

	it("is off until armed, then listens and closes the microphone on disarm", async () => {
		const manifest = await release();
		const { d, capture } = deps(manifest);
		const states: string[] = [];
		const service = new WakeService(
			{ manifestUrl: "/m", filesUrl: "/f" },
			{ onState: (s) => states.push(s) },
			d,
		);
		expect(service.getState()).toBe("off");
		await service.arm();
		expect(service.getState()).toBe("listening");
		expect(service.phrases).toEqual(["hey_oqto"]);
		expect(states).toEqual(["loading", "listening"]);
		await service.disarm();
		expect(service.getState()).toBe("off");
		expect(capture.close).toHaveBeenCalledOnce();
	});

	it("reports a detection once per refractory window, not per frame", async () => {
		const manifest = await release();
		const { d, engine, feed } = deps(manifest);
		const detections: string[] = [];
		const service = new WakeService(
			{ manifestUrl: "/m", filesUrl: "/f" },
			{ onDetection: (phrase) => detections.push(phrase) },
			d,
		);
		await service.arm();
		engine.push.mockResolvedValue([
			{ scores: { hey_oqto: 0.9 }, detections: ["hey_oqto"] },
			{ scores: { hey_oqto: 0.95 }, detections: ["hey_oqto"] },
		]);
		feed(new Float32Array(1280));
		await vi.waitFor(() => expect(detections).toEqual(["hey_oqto"]));
	});

	it("mutes the engine while the app speaks and resumes when the gate closes", async () => {
		const manifest = await release();
		const { d, engine } = deps(manifest);
		const service = new WakeService(
			{ manifestUrl: "/m", filesUrl: "/f" },
			{},
			d,
		);
		await service.arm();
		setAppPlayback(true);
		expect(engine.setPlayback).toHaveBeenLastCalledWith(true);
		expect(service.getState()).toBe("muted");
		setAppPlayback(false);
		expect(engine.setPlayback).toHaveBeenLastCalledWith(false);
		expect(service.getState()).toBe("listening");
		await service.disarm();
	});

	it("reports unavailable instead of crashing when there is no microphone", async () => {
		const manifest = await release();
		const { d } = deps(manifest, { capture: false });
		const states: string[] = [];
		const service = new WakeService(
			{ manifestUrl: "/m", filesUrl: "/f" },
			{ onState: (s) => states.push(s) },
			d,
		);
		await service.arm();
		expect(service.getState()).toBe("unavailable");
		expect(states).toEqual(["loading", "unavailable"]);
	});

	it("reports an error without throwing when the release cannot load", async () => {
		const manifest = await release({ mel: "f".repeat(64) });
		const { d } = deps(manifest);
		const errors: string[] = [];
		const service = new WakeService(
			{ manifestUrl: "/m", filesUrl: "/f" },
			{ onError: (m) => errors.push(m) },
			d,
		);
		await service.arm();
		expect(service.getState()).toBe("error");
		expect(errors[0]).toMatch(/manifest hash/);
	});
});
