/**
 * Voice mode UI components.
 */

export { VoiceModeButton } from "./VoiceModeButton";
export type { VoiceModeButtonProps } from "./VoiceModeButton";

export { LiveTranscript } from "./LiveTranscript";
export type { LiveTranscriptProps } from "./LiveTranscript";

export { VadProgressBar } from "./VadProgressBar";
export type { VadProgressBarProps } from "./VadProgressBar";

export { OrbVisualizer } from "./OrbVisualizer";
export type { OrbVisualizerProps } from "./OrbVisualizer";

export { KittVisualizer } from "./KittVisualizer";
export type { KittVisualizerProps } from "./KittVisualizer";

export { VoiceInputOverlay } from "./VoiceInputOverlay";
export type { VoiceInputOverlayProps } from "./VoiceInputOverlay";

export { VoicePanel } from "./VoicePanel";
export type { VoicePanelProps } from "./VoicePanel";

export { VoiceMenuButton, DEFAULT_VOICE_SHORTCUTS } from "./VoiceMenuButton";
export type { VoiceMode, VoiceShortcuts } from "./VoiceMenuButton";

export {
	DynamicVisualizer,
	getVisualizer,
	getAvailableVisualizers,
	isValidVisualizer,
	VISUALIZER_REGISTRY,
} from "./visualizers";
export type { VisualizerProps, VisualizerMeta } from "./visualizers";
