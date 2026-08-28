import { firstOperand, parseShellCommand } from "@/lib/shell-command";
import { describe, expect, it } from "vitest";

/** The parsed command word a reader would name for this line. */
function primaryOf(command: string): string | null {
	return parseShellCommand(command).primary?.name ?? null;
}

describe("shell command parsing", () => {
	it("ignores command words that only appear inside path arguments", () => {
		// The label for this line used to be "Running container command".
		expect(primaryOf("df -h / /var/lib/docker 2>/dev/null | uniq")).toBe("df");
		expect(primaryOf("du -sh ~/.local/share/podman")).toBe("du");
	});

	it("keeps the leading command when a later segment removes files", () => {
		// The label for this line used to be "Removing files".
		expect(
			primaryOf("df -h /tmp; probe=$(mktemp /tmp/p.XXXX) && rm -f $probe"),
		).toBe("df");
		expect(primaryOf("df -h .; git status --short")).toBe("df");
	});

	it("does not read commands out of heredoc bodies", () => {
		const command =
			"python3 - <<'PY'\nimport os\nos.system('rm -rf /tmp/x')\nPY";
		expect(primaryOf(command)).toBe("python3");
		expect(parseShellCommand(command).invocations).toHaveLength(1);
	});

	it("skips directory changes and narration to reach the real command", () => {
		expect(primaryOf("cd /repo && cargo test -p oqto")).toBe("cargo");
		expect(primaryOf("echo '=== build ===' && bun run build")).toBe("bun");
		expect(primaryOf("pwd && ls -la")).toBe("ls");
		expect(primaryOf("sleep 2; curl -s https://example.com")).toBe("curl");
	});

	it("falls back to the incidental command when it is all there is", () => {
		expect(primaryOf("cd /repo")).toBe("cd");
		expect(primaryOf("echo hello")).toBe("echo");
	});

	it("unwraps privilege and timing wrappers", () => {
		expect(primaryOf("sudo -u wismut systemctl --user restart oqto")).toBe(
			"systemctl",
		);
		expect(primaryOf("timeout 30 cargo build --release")).toBe("cargo");
		expect(primaryOf("DISPLAY=:0 nohup agent-browser open http://x")).toBe(
			"agent-browser",
		);
		expect(primaryOf("npx tsc --noEmit")).toBe("tsc");
	});

	it("resolves the command that ssh actually runs, and its host", () => {
		const quoted = parseShellCommand('ssh proxmox "rm -rf /opt/data/log"');
		expect(quoted.primary?.name).toBe("rm");
		expect(quoted.primary?.remoteHost).toBe("proxmox");

		const argv = parseShellCommand("ssh -t user@4090 systemctl status oqto");
		expect(argv.primary?.name).toBe("systemctl");
		expect(argv.primary?.remoteHost).toBe("4090");

		const bare = parseShellCommand("ssh octo-azure");
		expect(bare.primary?.name).toBe("ssh");
	});

	it("descends into loop bodies and command strings", () => {
		expect(primaryOf("for f in *.tmp; do rm $f; done")).toBe("rm");
		expect(primaryOf("bash -lc 'cd /tmp && ls -la'")).toBe("ls");
	});

	it("drops comments and redirection targets", () => {
		expect(primaryOf("# check the disk\ndf -h /")).toBe("df");
		expect(primaryOf("cargo build 2>&1 | tail -5")).toBe("cargo");
		expect(parseShellCommand("ls > out.txt").primary?.args).toEqual([]);
	});

	it("treats substituted command output as opaque, not as a command", () => {
		const parsed = parseShellCommand("kill $(pgrep -f oqto)");
		expect(parsed.primary?.name).toBe("kill");
	});

	it("reports every invocation in execution order", () => {
		expect(
			parseShellCommand(
				"cd /repo && git add -A && git commit -m 'x'",
			).invocations.map((invocation) => invocation.name),
		).toEqual(["cd", "git", "git"]);
	});

	it("returns nothing to describe for comment-only input", () => {
		expect(parseShellCommand("# just a note").primary).toBeNull();
		expect(parseShellCommand("").primary).toBeNull();
	});
});

describe("firstOperand", () => {
	it("skips flags and the arguments those flags consume", () => {
		expect(firstOperand(["-n", "50", "log.txt"], new Set(["-n"]))).toBe(
			"log.txt",
		);
		expect(firstOperand(["-la", "src"])).toBe("src");
		expect(firstOperand(["--noEmit"])).toBeNull();
	});
});
