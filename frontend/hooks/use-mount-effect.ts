import { type EffectCallback, useEffect } from "react";

/**
 * Runs an effect exactly once after mount.
 *
 * Prefer this hook over ad-hoc effects with empty dependency arrays so
 * behavior is explicit and searchable during refactors.
 */
export function useMountEffect(effect: EffectCallback): void {
	useEffect(effect, []);
}
