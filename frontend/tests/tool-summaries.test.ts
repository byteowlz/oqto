import { initI18n } from "@/lib/i18n";
import { getToolSummary } from "@/lib/tool-summaries";
import { beforeAll, describe, expect, it } from "vitest";

beforeAll(() => {
	initI18n();
});

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

	it("does not trigger cargo build/install pattern for rg searching cargo strings", () => {
		const summary = getToolSummary("bash", {
			command: 'rg -n "cargo install --path" justfile',
		});
		expect(summary).toBeTruthy();
		// Should hit the grep/rg pattern, not the cargo pattern
		expect(summary?.toLowerCase()).not.toContain("rust");
		expect(summary?.toLowerCase()).not.toContain("installing");
		expect(summary?.toLowerCase()).not.toContain("building");
	});

	it("shows checking for cargo check", () => {
		const summary = getToolSummary("bash", {
			command: "cd backend && cargo check -p oqto",
		});
		expect(summary).toBeTruthy();
		expect(summary?.toLowerCase()).toContain("check");
	});

	it("shows linting for cargo clippy", () => {
		const summary = getToolSummary("bash", {
			command: "cargo clippy -p oqto-runner",
		});
		expect(summary).toBeTruthy();
		expect(summary?.toLowerCase()).toContain("lint");
	});
});
