// Private source-only DEMO composition. Stock Pi/default local tools are never selected.
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { runClient } from "./client.ts";
import { runClientTui } from "./client-tui.ts";
import { startServer } from "./server.ts";

const args = process.argv.slice(2);
const smoke = args.includes("--smoke");
const coding = args.includes("--coding");
if (coding && args.includes("--demo"))
	throw new Error("Choose --coding or --demo, not both");
if ((process.env.OQT_MODE === "coding") !== coding)
	throw new Error("Launcher mode mismatch; refusing fallback");
const continuing = args.includes("--continue") || args.includes("-c");
if (
	args.some(
		(arg) =>
			!["--demo", "--coding", "--continue", "-c", "--smoke"].includes(arg),
	)
)
	throw new Error("Unsupported oqt prototype option");
if (!smoke && (!process.stdin.isTTY || !process.stdout.isTTY))
	throw new Error(
		"oqt requires a real terminal (use --smoke only for qualification)",
	);
const directory = process.env.OQT_SERVER_DIR;
if (!directory || !process.env.OQT_CARRIER || !process.env.OQT_SECCOMP_BPF)
	throw new Error("Missing protected demo launcher context; no fallback");
const packageRoot = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const plugin = join(packageRoot, "examples/plugins/oq-permissions");
const runtime = await startServer({
	directory,
	sessionDir: join(directory, "sessions"),
	pluginPackages: [plugin],
});
try {
	const command = {
		command: "client" as const,
		connect: { transport: "unix" as const, path: runtime.socketPath },
		pluginPackages: [plugin],
		...(continuing ? { continue: true } : {}),
	};
	if (smoke) {
		const result = await runClient({
			...command,
			prompt: coding
				? "Do not use tools. Reply exactly oqt-coding-ready."
				: "own history",
		});
		if (
			result.kind !== "prompted" ||
			!result.text.includes(coding ? "oqt-coding-ready" : "own fixture-token")
		)
			throw new Error(`Demo smoke failed: ${JSON.stringify(result)}`);
		console.log(`oqt ${coding ? "coding" : "demo"} smoke PASS: ${result.text}`);
	} else {
		await runClientTui(command, { directory });
	}
} finally {
	await runtime.close();
}
