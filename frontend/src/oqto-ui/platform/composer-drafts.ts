/** Disposable browser composer drafts, never durable Chat/history authority. */
export { appendCompletedDraft, writeChatDraft } from "@/lib/chat-draft-storage";

export function readChatDraft(key: string): string {
	try {
		return localStorage.getItem(key) ?? "";
	} catch {
		return "";
	}
}
