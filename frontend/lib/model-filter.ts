import { fuzzyMatch } from "@/lib/slash-commands";

export type ModelOption = {
	value: string;
	label: string;
};

export function filterModelOptions(
	options: ModelOption[],
	query: string,
): ModelOption[] {
	const trimmed = query.trim();
	if (!trimmed) return options;
	return options.filter((option) => {
		return (
			fuzzyMatch(trimmed, option.value) || fuzzyMatch(trimmed, option.label)
		);
	});
}
