import { getFeatures } from "@/lib/api/features";
import type { VoiceConfig } from "@/lib/voice/types";
import { useEffect, useState } from "react";

/**
 * Voice capability for hosts without a features provider (OqtoUI), so wake
 * phrases and remote dictation behave exactly as in the legacy UI. The legacy
 * UI already receives the same object through its features context.
 */
export function useVoiceFeatures(enabled: boolean): VoiceConfig | null {
	const [config, setConfig] = useState<VoiceConfig | null>(null);
	// useeffect-guardrail: allow — one bounded read of a capability document; no chat state.
	useEffect(() => {
		if (!enabled) return;
		let current = true;
		getFeatures()
			.then((features) => {
				if (current) setConfig((features.voice as VoiceConfig | null) ?? null);
			})
			.catch(() => {
				if (current) setConfig(null);
			});
		return () => {
			current = false;
		};
	}, [enabled]);
	return config;
}
