/**
 * Deterministic model/property-test harness for the OqtoUI compositor
 * (ADR-0041). No external property-testing dependency: sequences are
 * generated from fixed seeds, so every failure is reproducible from the
 * seed and the printed step log alone.
 */

export { SeededRng } from "../../scripts/compositor-traces/seeded-rng";
import { SeededRng } from "../../scripts/compositor-traces/seeded-rng";

export interface GeneratedRunConfig<Model, Step> {
	seeds: readonly number[];
	stepsPerSeed: number;
	initialModel: (rng: SeededRng) => Model;
	generateStep: (rng: SeededRng, model: Model) => Step;
	/** Applies one step and returns the next model; throws to fail the run. */
	applyStep: (model: Model, step: Step, rng: SeededRng) => Model;
}

/**
 * Runs generated step sequences for every seed. On failure, rethrows with
 * the seed, step index, and the full JSON step log so the exact sequence
 * can be replayed as a regression test.
 */
export function runGeneratedSequences<Model, Step>(
	config: GeneratedRunConfig<Model, Step>,
): void {
	for (const seed of config.seeds) {
		const rng = new SeededRng(seed);
		let model = config.initialModel(rng);
		const log: Step[] = [];
		for (let index = 0; index < config.stepsPerSeed; index += 1) {
			const step = config.generateStep(rng, model);
			log.push(step);
			try {
				model = config.applyStep(model, step, rng);
			} catch (error) {
				const detail = error instanceof Error ? error.message : String(error);
				throw new Error(
					`Generated sequence failed (seed=${seed}, step=${index}): ${detail}\nSteps: ${JSON.stringify(log)}`,
					{ cause: error },
				);
			}
		}
	}
}

/** Deep-freezes a value so accidental mutation throws in strict mode. */
export function deepFreeze<T>(value: T): T {
	if (value !== null && typeof value === "object" && !Object.isFrozen(value)) {
		Object.freeze(value);
		for (const key of Object.getOwnPropertyNames(value)) {
			deepFreeze((value as Record<string, object>)[key]);
		}
	}
	return value;
}
