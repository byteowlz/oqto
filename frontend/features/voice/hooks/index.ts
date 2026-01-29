/**
 * Voice feature hooks.
 *
 * This module exports all hooks related to voice functionality:
 * - Voice mode (full STT/TTS conversation)
 * - TTS (text-to-speech playback)
 * - Dictation (STT input mode)
 * - Voice commands (cross-component voice event system)
 */

export { useVoiceMode } from "./useVoiceMode";
export type {
	UseVoiceModeOptions,
	UseVoiceModeReturn,
} from "./useVoiceMode";

export { useTTS, useTTSWithParagraphs } from "./useTTS";
export type {
	TTSState,
	TTSSettings,
	UseTTSResult,
	UseTTSWithParagraphsResult,
} from "./useTTS";

export { useDictation } from "./useDictation";
export type {
	UseDictationOptions,
	UseDictationReturn,
} from "./useDictation";

export {
	emitVoiceCommand,
	useVoiceCommandListener,
	useVoiceCommandEmitter,
	useVoiceShortcuts,
	formatShortcut,
	VOICE_SHORTCUTS,
} from "./useVoiceCommands";
export type { VoiceCommandType } from "./useVoiceCommands";
