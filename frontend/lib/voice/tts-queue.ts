export type SpeakFn = (text: string) => Promise<void>;
export type IndexUpdateFn = (index: number) => void;
export type SessionCheckFn = (sessionId: number) => boolean;

export function splitIntoParagraphs(text: string): string[] {
	return text
		.split(/\n\n+/)
		.map((paragraph) => paragraph.trim())
		.filter((paragraph) => paragraph.length > 0);
}

export function createParagraphPlayer(
	paragraphs: string[],
	speak: SpeakFn,
	onIndexChange: IndexUpdateFn,
	isSessionActive: SessionCheckFn,
) {
	return {
		playFrom: async (index: number, sessionId: number): Promise<void> => {
			for (let i = index; i < paragraphs.length; i++) {
				if (!isSessionActive(sessionId)) {
					return;
				}
				onIndexChange(i);
				try {
					await speak(paragraphs[i]);
				} catch (error) {
					if (!isSessionActive(sessionId)) {
						return;
					}
					throw error;
				}
			}
		},
	};
}
