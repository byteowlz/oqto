// Design-only harness; all production surface imports remain in the fixture.
import { spawn } from "node:child_process";
import { mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import { createServer } from "vite";

const [fixturePath, outputPath, captureMode] = process.argv.slice(2);
if (captureMode && captureMode !== "--compact-chat")
	throw new Error("Unknown capture mode");
const compactCapture = captureMode === "--compact-chat";
if (!fixturePath || !outputPath)
	throw new Error(
		"Usage: render-session-comparison.ts fixture.json output-directory",
	);
const fixture = JSON.parse(await readFile(fixturePath, "utf8"));
if (fixture.schema !== "oqto.session-comparison-fixture/v1")
	throw new Error("Unknown comparison fixture schema");
await mkdir(outputPath, { recursive: true });
const server = await createServer({
	configFile: path.resolve("vite.config.ts"),
	server: { host: "127.0.0.1", port: 0, open: false },
	define: { __SESSION_COMPARISON_FIXTURE__: JSON.stringify(fixture) },
});
try {
	await server.listen();
	const address = server.httpServer?.address();
	if (!address || typeof address === "string")
		throw new Error("No disposable fixture listener");
	const child = spawn(
		"uv",
		[
			"run",
			"--no-project",
			"--with",
			"playwright==1.58.0",
			"python",
			compactCapture
				? "tests/capture-compact-chat.py"
				: "tests/capture-session-comparison.py",
			`http://127.0.0.1:${address.port}/tests/session-comparison-fixture.html`,
			path.resolve(outputPath),
			...(compactCapture ? [path.resolve(fixturePath)] : []),
		],
		{ stdio: "inherit" },
	);
	const result = await new Promise<number | null>((resolve, reject) => {
		child.once("exit", resolve);
		child.once("error", reject);
	});
	if (result !== 0) throw new Error("Web comparison capture failed");
} finally {
	await server.close();
}
