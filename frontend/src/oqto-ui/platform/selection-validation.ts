import type { JsonValue } from "../engine/projection";
import type {
	Answer,
	Answers,
	Choice,
	NormalizedSpec,
	Question,
	TextConstraint,
} from "./selection-contract";

function fail(message: string): never {
	throw new Error(message);
}
function object(value: unknown): Record<string, JsonValue | undefined> {
	if (
		!value ||
		typeof value !== "object" ||
		Array.isArray(value) ||
		![Object.prototype, null].includes(Object.getPrototypeOf(value))
	)
		fail("Expected plain object");
	const result = value as Record<string, JsonValue | undefined>;
	for (const key of Object.keys(result))
		if (["__proto__", "constructor", "prototype"].includes(key))
			fail("Unsafe key");
	return result;
}
function fields(
	value: Record<string, JsonValue | undefined>,
	allowed: readonly string[],
): void {
	for (const key of Object.keys(value))
		if (!allowed.includes(key)) fail(`Unknown field: ${key}`);
}
function text(value: unknown, maximum = 10000): string {
	if (typeof value !== "string" || value.length > maximum) fail("Invalid text");
	return value;
}
function optionalText(value: unknown, maximum = 10000): string | undefined {
	return value === undefined ? undefined : text(value, maximum);
}
function label(value: unknown): string {
	const result = text(value, 240);
	if (!result.trim()) fail("Empty label");
	return result;
}
function id(value: unknown): string {
	const result = text(value, 128);
	if (
		!/^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/.test(result) ||
		["constructor", "prototype", "__proto__"].includes(result)
	)
		fail("Invalid ID");
	return result;
}
function bool(value: unknown): boolean | undefined {
	if (value === undefined) return undefined;
	if (typeof value !== "boolean") fail("Invalid boolean");
	return value;
}
function bound(value: unknown): number | undefined {
	if (value === undefined) return undefined;
	if (
		typeof value !== "number" ||
		!Number.isSafeInteger(value) ||
		value < 0 ||
		value > 100000
	)
		fail("Invalid constraint");
	return value;
}
function list(value: unknown): unknown[] {
	if (!Array.isArray(value) || value.length > 1000) fail("Invalid list");
	return value;
}
function unique<T extends { id: string }>(items: T[]): T[] {
	if (new Set(items.map((item) => item.id)).size !== items.length)
		fail("Duplicate IDs");
	return items;
}
function choices(value: unknown): Choice[] {
	return unique(
		list(value).map((item) => {
			const row = object(item);
			fields(row, ["id", "label", "description", "recommended"]);
			return {
				id: id(row.id),
				label: label(row.label),
				description: optionalText(row.description),
				recommended: bool(row.recommended),
			};
		}),
	);
}
function note(value: unknown): TextConstraint | undefined {
	if (value === undefined) return undefined;
	const row = object(value);
	fields(row, ["label", "maxLength", "multiline"]);
	return {
		label: optionalText(row.label, 240),
		maxLength: bound(row.maxLength),
		multiline: bool(row.multiline),
	};
}
function question(value: unknown): Question {
	const row = object(value);
	fields(row, [
		"id",
		"title",
		"header",
		"description",
		"groupId",
		"kind",
		"options",
		"required",
		"allowOther",
		"minSelections",
		"maxSelections",
		"maxLength",
		"multiline",
		"note",
	]);
	if (!["single", "multiple", "text"].includes(row.kind as string))
		fail("Invalid kind");
	const result: Question = {
		id: id(row.id),
		title: label(row.title),
		header: optionalText(row.header, 40),
		description: optionalText(row.description),
		groupId: row.groupId === undefined ? undefined : id(row.groupId),
		kind: row.kind as Question["kind"],
		required: bool(row.required),
		allowOther: bool(row.allowOther),
		minSelections: bound(row.minSelections),
		maxSelections: bound(row.maxSelections),
		maxLength: bound(row.maxLength),
		multiline: bool(row.multiline),
		note: note(row.note),
	};
	if (result.kind === "text") {
		if (
			row.options !== undefined ||
			result.allowOther ||
			result.minSelections !== undefined ||
			result.maxSelections !== undefined
		)
			fail("Text cannot have choices");
		return result;
	}
	result.options = choices(row.options);
	if (!result.options.length) fail("Empty choices");
	const maximum =
		result.kind === "single"
			? 1
			: result.options.length + (result.allowOther ? 1 : 0);
	if (
		(result.minSelections ?? 0) > (result.maxSelections ?? maximum) ||
		(result.maxSelections ?? maximum) > maximum
	)
		fail("Invalid selection bounds");
	return result;
}
function reviewQuestions(
	row: Record<string, JsonValue | undefined>,
): Question[] {
	return unique(
		list(row.items).map((value) => {
			const item = object(value);
			fields(item, ["id", "label", "description", "groupId", "required"]);
			return question({
				id: item.id,
				title: item.label,
				description: item.description,
				groupId: item.groupId,
				required: item.required,
				kind: bool(row.multiple) ? "multiple" : "single",
				options: row.choices,
				allowOther: row.allowOther,
				note: row.note,
			});
		}),
	);
}
export function normalizeSpec(value: unknown): NormalizedSpec {
	const row = object(value);
	if (row.version !== 1) fail("Unsupported version");
	const common = ["version", "mode", "title", "description", "groups"];
	if (row.mode === "questions") fields(row, [...common, "questions"]);
	else if (row.mode === "review")
		fields(row, [
			...common,
			"items",
			"choices",
			"multiple",
			"allowOther",
			"note",
		]);
	else fail("Invalid mode");
	const groups = unique(
		list(row.groups ?? []).map((value) => {
			const group = object(value);
			fields(group, ["id", "title"]);
			return { id: id(group.id), title: label(group.title) };
		}),
	);
	const questions =
		row.mode === "questions"
			? unique(list(row.questions).map(question))
			: reviewQuestions(row);
	if (!questions.length) fail("Empty inventory");
	for (const entry of questions)
		if (entry.groupId && !groups.some((group) => group.id === entry.groupId))
			fail("Unknown group");
	return {
		version: 1,
		mode: row.mode,
		title: label(row.title),
		description: optionalText(row.description),
		questions,
		groups,
	};
}
export function validateNormalized(value: unknown): NormalizedSpec {
	const row = object(value);
	if (row.mode !== "questions" && row.mode !== "review") fail("Invalid mode");
	const result = normalizeSpec({ ...row, mode: "questions" });
	return { ...result, mode: row.mode };
}
export function emptyAnswers(spec: NormalizedSpec): Answers {
	return Object.fromEntries(
		spec.questions.map((entry) => [
			entry.id,
			{ answered: false, selectedIds: [] },
		]),
	);
}
function answerText(
	question: Question,
	value: unknown,
	supplemental: boolean,
): string | undefined {
	const constraint = supplemental ? question.note : question;
	const result = optionalText(value, constraint?.maxLength ?? 10000);
	if (result === undefined) return undefined;
	if (supplemental && !question.note) fail("Invalid note");
	if (!supplemental && question.kind !== "text" && !question.allowOther)
		fail("Invalid answer text");
	if (!constraint?.multiline && /[\r\n]/.test(result))
		fail(supplemental ? "Invalid note" : "Invalid answer text");
	return result;
}
function validateCompletion(
	question: Question,
	answered: boolean,
	count: number,
	value: string | undefined,
): void {
	if (answered && question.kind === "single" && count !== 1)
		fail("Empty answer");
	if (
		answered &&
		question.kind === "multiple" &&
		count < (question.minSelections ?? 0)
	)
		fail("Empty answer");
	if (
		answered &&
		question.kind === "text" &&
		question.required &&
		!value?.trim()
	)
		fail("Empty answer");
	if (question.required && !answered) fail("Required answer missing");
}
export function validateSelectionAnswer(
	question: Question,
	candidate: unknown,
	submit: boolean,
): Answer {
	const row = object(candidate);
	fields(row, ["answered", "selectedIds", "text", "note", "disposition"]);
	const answered = bool(row.answered);
	if (answered === undefined) fail("Missing answered");
	const selectedIds = list(row.selectedIds).map(id);
	if (
		new Set(selectedIds).size !== selectedIds.length ||
		selectedIds.some(
			(value) => !question.options?.some((option) => option.id === value),
		)
	)
		fail("Invalid choices");
	const value = answerText(question, row.text, false);
	const memo = answerText(question, row.note, true);
	const count =
		selectedIds.length + (question.kind !== "text" && value?.trim() ? 1 : 0);
	if (
		(question.kind === "text" && selectedIds.length) ||
		(question.kind === "single" && count > 1) ||
		count > (question.maxSelections ?? 1000)
	)
		fail("Too many selections");
	if (!answered && (selectedIds.length || value?.length))
		fail("Unanswered decision contains value");
	if (submit) validateCompletion(question, answered, count, value);
	if (
		row.disposition !== undefined &&
		(answered || !["skipped", "unsure"].includes(row.disposition as string))
	)
		fail("Invalid answer disposition");
	return {
		answered,
		selectedIds,
		...(value === undefined ? {} : { text: value }),
		...(memo === undefined ? {} : { note: memo }),
		...(row.disposition === undefined
			? {}
			: { disposition: row.disposition as Answer["disposition"] }),
	};
}
export function validateAnswers(
	spec: NormalizedSpec,
	value: unknown,
	submit: boolean,
): Answers {
	const row = object(value);
	for (const key of Object.keys(row))
		if (!spec.questions.some((entry) => entry.id === key))
			fail("Unknown answer");
	return Object.fromEntries(
		spec.questions.map((entry) => [
			entry.id,
			validateSelectionAnswer(
				entry,
				row[entry.id] ?? { answered: false, selectedIds: [] },
				submit,
			),
		]),
	);
}
