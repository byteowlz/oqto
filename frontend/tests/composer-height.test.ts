import { fitComposerHeight } from "@/features/chat/components/ChatView";
import { describe, expect, it } from "vitest";

function textareaWithContentHeight(px: number): HTMLTextAreaElement {
	const textarea = document.createElement("textarea");
	Object.defineProperty(textarea, "scrollHeight", { get: () => px });
	return textarea;
}

describe("composer height", () => {
	it("grows to its content (e.g. after dictation appends sentences)", () => {
		const textarea = textareaWithContentHeight(96);
		textarea.style.height = "36px";
		fitComposerHeight(textarea);
		expect(textarea.style.height).toBe("96px");
	});

	it("stops at the cap and scrolls beyond it", () => {
		const textarea = textareaWithContentHeight(900);
		fitComposerHeight(textarea);
		expect(textarea.style.height).toBe("200px");
	});
});
