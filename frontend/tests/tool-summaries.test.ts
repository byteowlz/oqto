import { getToolSummary } from "@/lib/tool-summaries";
import enMessages from "@/messages/en.json";
import i18next from "i18next";
import { beforeAll, describe, expect, it } from "vitest";

beforeAll(async () => {
	await i18next.init({
		resources: { en: { translation: enMessages } },
		lng: "en",
		fallbackLng: "en",
		interpolation: { escapeValue: false },
	});
});

function summarize(command: string): string | null {
	return getToolSummary("bash", { command });
}

describe("bash tool summaries", () => {
	it("never labels a command after a word that only appears in an argument", () => {
		expect(summarize("df -h / /var/lib/docker 2>/dev/null | uniq")).toBe(
			"Checking disk usage",
		);
		expect(summarize("df -h /tmp; probe=$(mktemp) && rm -f $probe")).toBe(
			"Checking disk usage",
		);
		expect(summarize("df -h .; git status --short")).toBe(
			"Checking disk usage",
		);
	});

	it("describes the command that ssh runs, prefixed by its host", () => {
		expect(summarize('ssh proxmox "rm -rf /opt/data/log"')).toBe(
			"proxmox: Removing files",
		);
		expect(summarize("ssh octo-azure systemctl status oqto")).toBe(
			"octo-azure: Managing service",
		);
	});

	it("reports the file being read, not a flag value", () => {
		expect(summarize("tail -n 50 /tmp/oqto-runner.log")).toBe(
			"Reading /tmp/oqto-runner.log",
		);
		expect(summarize("head -c 200 README.md")).toBe("Reading README.md");
		expect(summarize("cat -n src/main.rs")).toBe("Reading src/main.rs");
	});

	it("reports the search pattern, including explicit -e patterns", () => {
		expect(summarize("grep -rn 'fn main' src")).toBe("Searching for 'fn main'");
		expect(summarize("grep -e needle haystack.txt")).toBe(
			"Searching for 'needle'",
		);
		expect(summarize("rg -m 5 pattern src")).toBe("Searching for 'pattern'");
	});

	it("maps subcommands of known tools", () => {
		expect(summarize("cd /repo && git commit -m 'x'")).toBe(
			"Committing changes",
		);
		expect(summarize("cd /repo && cargo clippy -- -D warnings")).toBe(
			"Linting Rust project",
		);
		expect(summarize("bun run build")).toBe("Building project");
		expect(summarize("npx tsc --noEmit")).toBe("Checking types");
	});

	it("shows the literal command when the tool is not recognized", () => {
		// An honest command beats an invented description.
		expect(summarize("mmdc -i diagram.mmd -o out.svg")).toBe(
			"mmdc -i diagram.mmd",
		);
		expect(summarize("git rev-parse --is-inside-work-tree")).toBe(
			"git rev-parse --is-inside-work-tree",
		);
		expect(summarize("cargo metadata --format-version 1")).toBe(
			"cargo metadata --format-version",
		);
	});

	it("keeps labels short", () => {
		const label = summarize(
			"weird-tool --a=/very/long/path/one --b=/very/long/path/two --c=/three",
		);
		expect(label).not.toBeNull();
		expect((label as string).length).toBeLessThanOrEqual(44);
	});

	it("has no label for input that runs nothing", () => {
		expect(summarize("# just a note")).toBeNull();
	});

	it("still labels streaming calls before the command arrives", () => {
		expect(getToolSummary("bash", {})).toBe("Running command");
	});
});
