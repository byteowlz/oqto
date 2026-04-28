import { getRunnerHistoryAlias } from "@/components/contexts/chat-context";

export const areSessionIdsEquivalent = (
	a: string | null | undefined,
	b: string | null | undefined,
): boolean => {
	if (!a || !b) return false;
	if (a === b) return true;

	const aliasA = getRunnerHistoryAlias(a);
	const aliasB = getRunnerHistoryAlias(b);
	return aliasA === b || aliasB === a || (Boolean(aliasA) && aliasA === aliasB);
};

export const createClientId = (): string =>
	typeof crypto !== "undefined" && "randomUUID" in crypto
		? crypto.randomUUID()
		: `${Date.now()}-${Math.random().toString(36).slice(2)}`;
