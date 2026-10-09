/**
 * Deterministic mulberry32 RNG shared by the compositor conformance-trace
 * generator and the model/property tests. The algorithm is part of the
 * trace-corpus contract only insofar as the committed JSON is the truth: a
 * second host consumes the traces, never this generator.
 */
export class SeededRng {
	private state: number;

	constructor(seed: number) {
		this.state = seed >>> 0 || 0x9e3779b9;
	}

	/** mulberry32: returns a float in [0, 1). */
	next(): number {
		this.state = (this.state + 0x6d2b79f5) >>> 0;
		let t = this.state;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	}

	int(maxExclusive: number): number {
		return Math.floor(this.next() * maxExclusive);
	}

	pick<T>(items: readonly T[]): T {
		if (items.length === 0) {
			throw new Error("SeededRng.pick called with an empty list");
		}
		return items[this.int(items.length)];
	}

	bool(probability = 0.5): boolean {
		return this.next() < probability;
	}
}
