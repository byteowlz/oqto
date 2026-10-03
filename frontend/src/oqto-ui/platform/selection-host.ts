import type { Answers, SelectionRecord } from "./selection-contract";
export type SelectionSnapshot = {
	phase: "connecting" | "ready" | "unavailable";
	record: SelectionRecord | null;
	answers: Answers;
	dirty: boolean;
	busy: boolean;
	error: string | null;
	fieldErrors: Record<string, string>;
	presentationEpoch: number;
};
export type SelectionController = {
	subscribe(listener: () => void): () => void;
	snapshot(): SelectionSnapshot;
	edit(answers: Answers): void;
	save(submit?: boolean): Promise<void>;
	reload(id?: string): Promise<void>;
	close(): Promise<void>;
	rendered(reviewId: string): void;
};
