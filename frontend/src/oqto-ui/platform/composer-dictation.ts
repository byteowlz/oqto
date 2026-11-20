/** Browser capture and disposable composer drafts live at the platform boundary.
 * Chat continues to submit exclusively through platform.chat; speech writes no history.
 */
export {
	DictationMicButton,
	DictationSettingsPanel,
	DictationStatus,
} from "@/components/voice/ComposerDictation";
export { useDictation } from "@/features/voice/hooks/useDictation";
export { useVoiceFeatures } from "@/features/voice/hooks/useVoiceFeatures";
