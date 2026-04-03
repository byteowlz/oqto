import { getToolSummary } from "@/lib/tool-summaries";
import { describe, expect, it } from "vitest";

describe("tool summary labels", () => {
	it("summarizes agent-browser snapshot commands with env prefixes", () => {
		const summary = getToolSummary("bash", {
			command: "DISPLAY=:0 agent-browser snapshot -i",
		});
		expect(summary).toBeTruthy();
		expect(summary?.toLowerCase()).toContain("snapshot");
	});

	it("summarizes agent-browser click commands", () => {
		const summary = getToolSummary("bash", {
			command: "agent-browser click @e2",
		});
		expect(summary).toBeTruthy();
		expect(summary?.toLowerCase()).toContain("browser");
	});

	it("still returns a readable fallback for non-browser bash commands", () => {
		const summary = getToolSummary("bash", {
			command: 'rg -n "rename" frontend/src',
		});
		expect(summary).toBeTruthy();
		expect(summary?.length).toBeGreaterThan(0);
	});
});
