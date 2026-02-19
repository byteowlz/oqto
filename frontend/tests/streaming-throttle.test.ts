import { describe, expect, it, vi } from "vitest";
import { createStreamingThrottle } from "../lib/streaming-throttle";

describe("createStreamingThrottle", () => {
	it("emits the first value immediately", () => {
		const throttle = createStreamingThrottle<string>(80);
		const result = throttle.offer("hello");
		expect(result).toBe("hello");
		expect(throttle.hasPending()).toBe(false);
	});

	it("coalesces values within the interval", () => {
		vi.useFakeTimers();
		const throttle = createStreamingThrottle<string>(80);

		// First offer emits immediately
		expect(throttle.offer("a")).toBe("a");

		// Subsequent offers within the interval are coalesced
		expect(throttle.offer("ab")).toBeNull();
		expect(throttle.offer("abc")).toBeNull();
		expect(throttle.hasPending()).toBe(true);

		vi.useRealTimers();
	});

	it("drainReady returns null before interval elapses", () => {
		vi.useFakeTimers();
		const throttle = createStreamingThrottle<string>(80);

		throttle.offer("a"); // immediate
		throttle.offer("ab"); // coalesced

		// Not enough time has passed
		vi.advanceTimersByTime(50);
		expect(throttle.drainReady()).toBeNull();

		vi.useRealTimers();
	});

	it("drainReady returns pending value after interval elapses", () => {
		vi.useFakeTimers();
		const throttle = createStreamingThrottle<string>(80);

		throttle.offer("a"); // immediate
		throttle.offer("ab"); // coalesced

		vi.advanceTimersByTime(80);
		expect(throttle.drainReady()).toBe("ab");
		expect(throttle.hasPending()).toBe(false);

		vi.useRealTimers();
	});

	it("flush returns pending value regardless of timing", () => {
		vi.useFakeTimers();
		const throttle = createStreamingThrottle<string>(80);

		throttle.offer("a"); // immediate
		throttle.offer("ab"); // coalesced

		// Flush before interval - should still return the value
		const flushed = throttle.flush();
		expect(flushed).toBe("ab");
		expect(throttle.hasPending()).toBe(false);

		vi.useRealTimers();
	});

	it("flush returns null when nothing pending", () => {
		const throttle = createStreamingThrottle<string>(80);
		throttle.offer("a"); // immediate, nothing pending
		expect(throttle.flush()).toBeNull();
	});

	it("reset clears all state", () => {
		vi.useFakeTimers();
		const throttle = createStreamingThrottle<string>(80);

		throttle.offer("a");
		throttle.offer("ab");
		expect(throttle.hasPending()).toBe(true);

		throttle.reset();
		expect(throttle.hasPending()).toBe(false);
		expect(throttle.flush()).toBeNull();

		// After reset, next offer should emit immediately
		expect(throttle.offer("fresh")).toBe("fresh");

		vi.useRealTimers();
	});

	it("emits again after enough time passes", () => {
		vi.useFakeTimers();
		const throttle = createStreamingThrottle<string>(80);

		expect(throttle.offer("a")).toBe("a");
		expect(throttle.offer("ab")).toBeNull();

		vi.advanceTimersByTime(80);

		// Now the next offer should emit immediately
		expect(throttle.offer("abc")).toBe("abc");

		vi.useRealTimers();
	});

	it("coalesces to the latest value only", () => {
		vi.useFakeTimers();
		const throttle = createStreamingThrottle<number>(100);

		expect(throttle.offer(1)).toBe(1); // immediate
		expect(throttle.offer(2)).toBeNull(); // coalesced
		expect(throttle.offer(3)).toBeNull(); // coalesced (replaces 2)
		expect(throttle.offer(4)).toBeNull(); // coalesced (replaces 3)

		vi.advanceTimersByTime(100);
		expect(throttle.drainReady()).toBe(4); // only the latest

		vi.useRealTimers();
	});
});
