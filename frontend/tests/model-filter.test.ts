import { describe, expect, it } from "vitest";

import { filterModelOptions } from "@/lib/model-filter";

describe("filterModelOptions", () => {
	const options = [
		{ value: "openai/gpt-4o", label: "openai/gpt-4o · GPT-4o" },
		{ value: "anthropic/claude-3-5-sonnet", label: "claude-3.5-sonnet" },
		{ value: "local/qwen2.5-72b", label: "qwen2.5-72b" },
	];

	it("returns all options when query is empty", () => {
		expect(filterModelOptions(options, "")).toEqual(options);
		expect(filterModelOptions(options, "   ")).toEqual(options);
	});

	it("matches by provider/model value", () => {
		expect(filterModelOptions(options, "gpt4")).toEqual([options[0]]);
	});

	it("matches by label text", () => {
		expect(filterModelOptions(options, "sonet")).toEqual([options[1]]);
	});

	it("returns empty when no matches", () => {
		expect(filterModelOptions(options, "mistral")).toEqual([]);
	});
});
