/**
 * Features API
 * Feature flags and configuration
 */

import { authFetch, controlPlaneApiUrl } from "./client";

// ============================================================================
// Features Types
// ============================================================================

/** Per-visualizer voice settings from backend */
export type VisualizerVoiceConfig = {
	voice: string;
	speed: number;
};

/** Voice configuration from backend */
export type VoiceFeatureConfig = {
	stt_url: string;
	tts_url: string;
	vad_timeout_ms: number;
	default_voice: string;
	default_speed: number;
	auto_language_detect: boolean;
	tts_muted: boolean;
	continuous_mode: boolean;
	default_visualizer: string;
	interrupt_word_count: number;
	interrupt_backoff_ms: number;
	visualizer_voices: Record<string, VisualizerVoiceConfig>;
};

export type SessionAutoAttachMode = "off" | "attach" | "resume";

export type Features = {
	mmry_enabled: boolean;
	session_auto_attach?: SessionAutoAttachMode;
	session_auto_attach_scan?: boolean;
	/** Voice configuration (present if voice mode is enabled) */
	voice?: VoiceFeatureConfig | null;
	/** Use WebSocket for real-time events instead of SSE */
	websocket_events?: boolean;
	/** Whether the agent-browser integration is enabled */
	agent_browser_enabled?: boolean;
};

// ============================================================================
// Features API
// ============================================================================

export async function getFeatures(): Promise<Features> {
	const res = await authFetch(controlPlaneApiUrl("/api/features"), {
		credentials: "include",
	});
	if (!res.ok) {
		// Return defaults if endpoint not available
		return {
			mmry_enabled: false,
			voice: null,
			session_auto_attach: "off",
			session_auto_attach_scan: false,
		};
	}
	return res.json();
}
