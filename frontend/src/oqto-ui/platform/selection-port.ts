export type SelectionOperation =
	| "presentation_bind"
	| "presentation_unbind"
	| "selection_get"
	| "selection_save"
	| "selection_submit"
	| "selection_close"
	| "selection_ack";
export type SelectionPort = {
	discover(): Promise<boolean>;
	control(
		operation: SelectionOperation,
		argumentsValue: object,
	): Promise<unknown>;
	listen(
		present: (value: unknown) => void,
		connection: (connected: boolean) => void,
	): () => void;
	renew(callback: () => void): () => void;
	resume(scopeId: string): string | null;
	remember(scopeId: string, reviewId: string): void;
};
