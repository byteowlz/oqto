/** Session-owned execution bindings. Names select authority-bound objects, not
 * network addresses supplied by a model. Replacement never retargets an in-flight
 * call: its captured binding must still be current before delivery. */
export interface ExecutionBinding<T> {
	readonly id: string;
	readonly generation: number;
	readonly environment: T;
}

export class ExecutionBindings<T> {
	readonly #bindings = new Map<string, ExecutionBinding<T>>();
	readonly #generations = new Map<string, number>();

	bind(id: string, environment: T): ExecutionBinding<T> {
		if (!/^[a-z][a-z0-9-]{0,47}$/.test(id))
			throw new Error("Invalid environment name");
		const generation = (this.#generations.get(id) ?? 0) + 1;
		const binding = Object.freeze({ id, generation, environment });
		this.#generations.set(id, generation);
		this.#bindings.set(id, binding);
		return binding;
	}

	revoke(id: string): void {
		this.#bindings.delete(id);
	}

	capture(id: string): ExecutionBinding<T> {
		const binding = this.#bindings.get(id);
		if (!binding) throw new Error(`Execution environment unavailable: ${id}`);
		return binding;
	}

	check(binding: ExecutionBinding<T>): void {
		if (this.#bindings.get(binding.id) !== binding)
			throw new Error(
				`Execution binding retired: ${binding.id}/${binding.generation}; effects may be uncertain`,
			);
	}

	list(): readonly ExecutionBinding<T>[] {
		return [...this.#bindings.values()];
	}
}
