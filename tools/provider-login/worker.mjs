#!/usr/bin/env node
import { once } from "node:events";
import { isAbsolute } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import { ProviderLoginBroker } from "./broker.mjs";
import { createPiLoginRuntime } from "./pi-runtime.mjs";

export async function dispatch(broker, actor, request) {
	if (!request || typeof request !== "object" || Array.isArray(request))
		throw new Error("Invalid request");
	const fields = {
		providers: [],
		start: ["provider", "method"],
		status: ["attempt"],
		cancel: ["attempt"],
		commit: ["attempt", "nonce"],
		answer: ["attempt", "prompt", "value"],
	};
	const allowed = Object.hasOwn(fields, request.command)
		? fields[request.command]
		: null;
	if (
		!allowed ||
		Object.keys(request).some(
			(key) => !["id", "command", ...allowed].includes(key),
		)
	)
		throw new Error("Invalid request");
	if (
		typeof request.id !== "string" ||
		!/^[a-zA-Z0-9_-]{1,80}$/.test(request.id)
	)
		throw new Error("Invalid request");
	for (const field of allowed)
		if (typeof request[field] !== "string" || request[field].length > 8192)
			throw new Error("Invalid request");
	switch (request.command) {
		case "providers":
			return broker.providers(actor);
		case "start":
			return broker.start(actor, request.provider, request.method);
		case "status":
			return broker.status(actor, request.attempt);
		case "answer":
			return broker.answer(
				actor,
				request.attempt,
				request.prompt,
				request.value,
			);
		case "cancel":
			return broker.cancel(actor, request.attempt);
		case "commit":
			return broker.commit(actor, request.attempt, request.nonce);
	}
}
async function main() {
	const args = process.argv.slice(2);
	if (args.length === 1 && args[0] === "--help") {
		process.stdout.write(
			"Usage: worker.mjs --sdk /absolute/pi/dist/index.js --agent-dir /absolute/managed/.pi/agent --machine ID --account ID\nPrivate LF-JSONL stdin/stdout; see README.md. Never expose as an unauthenticated network service.\n",
		);
		return;
	}
	if (args.length !== 8) throw new Error("Invalid arguments");
	const options = new Map();
	for (let i = 0; i < args.length; i += 2) {
		if (
			!["--sdk", "--agent-dir", "--machine", "--account"].includes(args[i]) ||
			options.has(args[i])
		)
			throw new Error("Invalid arguments");
		options.set(args[i], args[i + 1]);
	}
	if (
		!isAbsolute(options.get("--sdk")) ||
		!isAbsolute(options.get("--agent-dir"))
	)
		throw new Error("Absolute managed paths required");
	const actor = options.get("--account");
	const machine = options.get("--machine");
	if (
		!/^[a-zA-Z0-9_-]{1,128}$/.test(actor) ||
		!/^[a-zA-Z0-9_-]{1,128}$/.test(machine)
	)
		throw new Error("Invalid owner");
	// Provider diagnostics must never contaminate the wire or disclose credentials.
	console.log =
		console.info =
		console.warn =
		console.error =
		console.debug =
			() => {};
	const runtime = await createPiLoginRuntime(
		await import(pathToFileURL(options.get("--sdk")).href),
		options.get("--agent-dir"),
	);
	const broker = new ProviderLoginBroker({
		runtime,
		machine,
		authorize: async (caller) => caller === actor,
		requireCommit: true,
	});
	process.once("SIGTERM", () => {
		broker.close();
		process.exit(0);
	});
	process.once("SIGINT", () => {
		broker.close();
		process.exit(0);
	});
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let pending = 0;
	let chain = Promise.resolve();
	process.stdin.on("data", (chunk) => {
		buffer += decoder.write(chunk);
		if (Buffer.byteLength(buffer) > 16_384) {
			broker.close();
			process.exitCode = 1;
			process.stdin.destroy();
			return;
		}
		for (;;) {
			const newline = buffer.indexOf("\n");
			if (newline < 0) break;
			const line = buffer.slice(0, newline);
			buffer = buffer.slice(newline + 1);
			if (++pending > 32) {
				broker.close();
				process.exitCode = 1;
				process.stdin.destroy();
				return;
			}
			chain = chain
				.then(async () => {
					let id = null;
					let response;
					try {
						const request = JSON.parse(line);
						if (
							typeof request?.id === "string" &&
							/^[a-zA-Z0-9_-]{1,80}$/.test(request.id)
						)
							id = request.id;
						response = {
							id,
							ok: true,
							data: await dispatch(broker, actor, request),
						};
					} catch {
						response = {
							id,
							ok: false,
							error:
								"Authentication operation rejected; check ownership, attempt state, and managed Pi support",
						};
					}
					if (!process.stdout.write(`${JSON.stringify(response)}\n`))
						await once(process.stdout, "drain");
					pending--;
				})
				.catch(() => {
					broker.close();
					process.exitCode = 1;
					process.stdin.destroy();
				});
		}
	});
	process.stdin.once("end", () => {
		void chain.finally(() => {
			broker.close();
			if (buffer.length) process.exitCode = 1;
		});
	});
}
if (
	process.argv[1] &&
	import.meta.url === pathToFileURL(process.argv[1]).href
) {
	void main().catch(() => {
		process.stderr.write(
			"Provider login worker unavailable; check managed Pi SDK and private configuration paths\n",
		);
		process.exitCode = 1;
	});
}
