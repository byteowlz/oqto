export type ParsedPiSessionStats = {
	input: number;
	output: number;
	contextWindowLength: number;
};

export const normalizeTokenCount = (value: unknown): number => {
	const n = typeof value === "number" ? value : Number(value);
	if (!Number.isFinite(n) || n <= 0) return 0;
	return Math.floor(n);
};

const readNumberFromCandidates = (
	source: Record<string, unknown>,
	keys: string[],
): number => {
	for (const key of keys) {
		if (!(key in source)) continue;
		const value = normalizeTokenCount(source[key]);
		if (value > 0) return value;
	}
	return 0;
};

export const parsePiSessionStats = (stats: unknown): ParsedPiSessionStats => {
	if (!stats || typeof stats !== "object") {
		return { input: 0, output: 0, contextWindowLength: 0 };
	}

	const statsRecord = stats as Record<string, unknown>;
	const tokensRecord =
		statsRecord.tokens && typeof statsRecord.tokens === "object"
			? (statsRecord.tokens as Record<string, unknown>)
			: null;

	const input = tokensRecord
		? readNumberFromCandidates(tokensRecord, ["input", "input_tokens"])
		: 0;
	const output = tokensRecord
		? readNumberFromCandidates(tokensRecord, ["output", "output_tokens"])
		: 0;
	const contextWindowLength = readNumberFromCandidates(statsRecord, [
		"contextWindowLength",
		"context_window_length",
		"contextLength",
		"context_length",
	]);

	return { input, output, contextWindowLength };
};
