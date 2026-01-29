/**
 * Voice feature module.
 *
 * Provides voice mode functionality including:
 * - Voice conversation mode (STT + TTS)
 * - Dictation mode (STT only)
 * - Text-to-speech playback
 * - Voice commands event system
 *
 * Components are re-exported from components/voice for backwards compatibility.
 */

// Hooks
export {
	// Voice mode
	useVoiceMode,
	type UseVoiceModeOptions,
	type UseVoiceModeReturn,
	// TTS
	useTTS,
	useTTSWithParagraphs,
	type TTSState,
	type TTSSettings,
	type UseTTSResult,
	type UseTTSWithParagraphsResult,
	// Dictation
	useDictation,
	type UseDictationOptions,
	type UseDictationReturn,
	// Voice commands
	emitVoiceCommand,
	useVoiceCommandListener,
	useVoiceCommandEmitter,
	useVoiceShortcuts,
	formatShortcut,
	VOICE_SHORTCUTS,
	type VoiceCommandType,
} from "./hooks";

// Re-export components from components/voice for convenience
export {
	VoiceModeButton,
	type VoiceModeButtonProps,
	LiveTranscript,
	type LiveTranscriptProps,
	VadProgressBar,
	type VadProgressBarProps,
	OrbVisualizer,
	type OrbVisualizerProps,
	KittVisualizer,
	type KittVisualizerProps,
	VoiceInputOverlay,
	type VoiceInputOverlayProps,
	DictationOverlay,
	type DictationOverlayProps,
	VoicePanel,
	type VoicePanelProps,
	VoiceMenuButton,
	DEFAULT_VOICE_SHORTCUTS,
	type VoiceMode,
	type VoiceShortcuts,
	DynamicVisualizer,
	getVisualizer,
	getAvailableVisualizers,
	isValidVisualizer,
	VISUALIZER_REGISTRY,
	type VisualizerProps,
	type VisualizerMeta,
} from "@/components/voice";
