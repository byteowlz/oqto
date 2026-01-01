/**
 * Voice mode services for real-time speech-to-text and text-to-speech.
 *
 * Uses:
 * - eaRS for STT (speech-to-text with VAD)
 * - kokorox for TTS (text-to-speech with streaming)
 */

export { STTService } from "./stt-service";
export type { MicrophoneDevice, STTCallbacks } from "./stt-service";

export { TTSService } from "./tts-service";
export type { TTSCallbacks } from "./tts-service";

export type { VoiceConfig } from "./types";
