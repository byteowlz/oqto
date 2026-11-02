import type { ToolRegistration } from "@earendil-works/pi-durable";
import type {
	ExecutionBinding,
	ExecutionBindings,
} from "./execution-bindings.ts";
import { type CodingGate, gatedTool } from "./gated-tool.ts";

/** Owner-constructed gate for exactly one environment. The model cannot supply
 * an address, credentials, cwd binding or fallback gate. */
export interface GuardedEnvironment {
	readonly gate: CodingGate;
	readonly description: string;
}

/** Named tools keep the tool's original input schema intact. The guard still
 * receives the underlying tool name (bash/read/edit/write), while the model sees
 * e.g. linux_bash. Each environment must have its own guard-side authority. */
export function environmentTools(
	tools: readonly ToolRegistration[],
	bindings: ExecutionBindings<GuardedEnvironment>,
	session: string,
): ToolRegistration[] {
	return bindings.list().flatMap((binding) =>
		tools.map((tool) => {
			const guarded = gatedTool(tool, fencedGate(bindings, binding), session);
			return {
				...guarded,
				name: `${binding.id.replaceAll("-", "_")}_${tool.name}`,
				description: `[Environment: ${binding.id}] ${binding.environment.description}\n${tool.description}`,
			};
		}),
	);
}

function fencedGate(
	bindings: ExecutionBindings<GuardedEnvironment>,
	binding: ExecutionBinding<GuardedEnvironment>,
): CodingGate {
	const gate = binding.environment.gate;
	return {
		async prepare(operation, context) {
			bindings.check(binding);
			await gate.prepare(operation, context);
		},
		async check(operation, context) {
			bindings.check(binding);
			await gate.check(operation, context);
			bindings.check(binding);
		},
		finish: (operation) => gate.finish(operation),
		...(gate.env ? { env: (operation) => gate.env?.(operation) } : {}),
		...(gate.budget
			? { budget: (operation) => gate.budget?.(operation) ?? 0 }
			: {}),
		...(gate.drain
			? { drain: (operation) => gate.drain?.(operation) ?? Promise.resolve() }
			: {}),
	};
}
