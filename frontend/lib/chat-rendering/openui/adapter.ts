/** Host-neutral data normalization only; no transport, state, actions or React. */
export interface OpenUIPartInput {
	program: string;
	fallback?: string;
	wait?: boolean;
}

type ToolInputFields = {
	program?: unknown;
	fallback?: unknown;
	wait?: unknown;
};

/** The closed profile parser, not this decoder, owns the program schema. */
export function readOpenUIPart(raw: unknown): OpenUIPartInput | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as ToolInputFields;
	if (typeof value.program !== "string" || !value.program.trim()) return null;
	return {
		program: value.program,
		fallback: typeof value.fallback === "string" ? value.fallback : undefined,
		wait: value.wait === true,
	};
}

/** Show only the answer actually reported by the native tool, never a local echo. */
export function readOpenUIAnswerText(output: unknown): string | undefined {
	if (!output || typeof output !== "object") return undefined;
	const details = (output as { details?: unknown }).details;
	if (!details || typeof details !== "object") return undefined;
	const reported = details as { outcome?: unknown; answer?: unknown };
	if (reported.outcome !== "answered") return undefined;
	try {
		return JSON.stringify(reported.answer);
	} catch {
		return undefined;
	}
}
