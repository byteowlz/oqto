/** Browser capture and disposable composer drafts live at the platform boundary.
 * Chat continues to submit exclusively through platform.chat; speech writes no history.
 */
export {
	DictationMicButton,
	DictationStatus,
} from "@/components/voice/ComposerDictation";
export { useDictation } from "@/features/voice/hooks/useDictation";
export { appendCompletedDraft, writeChatDraft } from "@/lib/chat-draft-storage";

export function readChatDraft(key: string): string {
	try {
		return localStorage.getItem(key) ?? "";
	} catch {
		return "";
	}
}
