#!/usr/bin/env node
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";

function usage() {
	console.log(`
Runner CLI (JSON over Unix socket)

Usage:
  node scripts/runner-cli.mjs <command> [args]

Commands:
  ping
  pi-list
  pi-get-state <session_id>
  pi-get-messages <session_id>
  pi-get-commands <session_id>
  pi-create <session_id> --cwd <path> [--provider p] [--model m] [--session-file f] [--continue-session f] [--env KEY=VAL]
  pi-prompt <session_id> <message...>
  pi-steer <session_id> <message...>
  pi-follow-up <session_id> <message...>
  pi-abort <session_id>
  pi-close <session_id>

Options:
  --socket <path>     Override runner socket path
  --raw <json>        Send raw JSON request
  --file <path>       Send JSON request from file
  --pretty            Pretty-print JSON output

Environment:
  OQTO_RUNNER_SOCKET  Default socket path override
`);
}

function defaultSocketPath() {
	const envSocket = process.env.OQTO_RUNNER_SOCKET || process.env.OCTO_RUNNER_SOCKET;
	if (envSocket) return envSocket;
	const user = process.env.USER || "";
	if (user) {
		const candidate = `/run/oqto/runner-sockets/${user}/oqto-runner.sock`;
		if (fs.existsSync(candidate)) return candidate;
	}
	const uid = typeof process.getuid === "function" ? String(process.getuid()) : "";
	if (uid) {
		const candidate = `/run/user/${uid}/oqto-runner.sock`;
		if (fs.existsSync(candidate)) return candidate;
	}
	return "/tmp/oqto-runner.sock";
}

function parseArgs(argv) {
	const args = [...argv];
	const flags = new Map();
	const positionals = [];

	while (args.length > 0) {
		const next = args.shift();
		if (!next) continue;
		if (next.startsWith("--")) {
			const key = next.slice(2);
			const value = args.length > 0 && !args[0].startsWith("--") ? args.shift() : true;
			flags.set(key, value);
			continue;
		}
		positionals.push(next);
	}

	return { flags, positionals };
}

function buildPiConfig(flags) {
	const config = {
		cwd: flags.get("cwd") || process.cwd(),
		provider: flags.get("provider") || null,
		model: flags.get("model") || null,
		session_file: flags.get("session-file") || null,
		continue_session: flags.get("continue-session") || null,
		env: {},
	};

	const envFlags = flags.get("env");
	const envList = Array.isArray(envFlags) ? envFlags : envFlags ? [envFlags] : [];
	for (const entry of envList) {
		if (typeof entry !== "string") continue;
		const idx = entry.indexOf("=");
		if (idx === -1) continue;
		const key = entry.slice(0, idx).trim();
		const value = entry.slice(idx + 1).trim();
		if (key) config.env[key] = value;
	}

	return config;
}

function buildRequest(positionals, flags) {
	const command = positionals[0];
	if (!command) return null;

	switch (command) {
		case "ping":
			return { type: "ping" };
		case "pi-list":
			return { type: "pi_list_sessions" };
		case "pi-get-state":
			return { type: "pi_get_state", session_id: positionals[1] };
		case "pi-get-messages":
			return { type: "pi_get_messages", session_id: positionals[1] };
		case "pi-get-commands":
			return { type: "pi_get_commands", session_id: positionals[1] };
		case "pi-create": {
			const sessionId = positionals[1];
			const config = buildPiConfig(flags);
			return { type: "pi_create_session", session_id: sessionId, config };
		}
		case "pi-prompt":
			return {
				type: "pi_prompt",
				session_id: positionals[1],
				message: positionals.slice(2).join(" "),
			};
		case "pi-steer":
			return {
				type: "pi_steer",
				session_id: positionals[1],
				message: positionals.slice(2).join(" "),
			};
		case "pi-follow-up":
			return {
				type: "pi_follow_up",
				session_id: positionals[1],
				message: positionals.slice(2).join(" "),
			};
		case "pi-abort":
			return { type: "pi_abort", session_id: positionals[1] };
		case "pi-close":
			return { type: "pi_close_session", session_id: positionals[1] };
		default:
			return null;
	}
}

function sendRequest(socketPath, request, pretty) {
	return new Promise((resolve, reject) => {
		const socket = net.createConnection(socketPath);
		let buffer = "";

		socket.on("connect", () => {
			socket.write(`${JSON.stringify(request)}\n`);
		});

		socket.on("data", (chunk) => {
			buffer += chunk.toString("utf-8");
			const idx = buffer.indexOf("\n");
			if (idx === -1) return;
			const line = buffer.slice(0, idx).trim();
			socket.end();
			if (!line) return resolve(null);
			try {
				const parsed = JSON.parse(line);
				resolve(parsed);
			} catch (err) {
				reject(err);
			}
		});

		socket.on("error", (err) => reject(err));
	});
}

async function main() {
	const { flags, positionals } = parseArgs(process.argv.slice(2));
	if (positionals.length === 0 && !flags.has("raw") && !flags.has("file")) {
		usage();
		process.exit(1);
	}

	const socketPath = flags.get("socket") || defaultSocketPath();
	if (!socketPath) {
		console.error("No socket path resolved.");
		process.exit(1);
	}

	let request;
	if (flags.has("raw")) {
		request = JSON.parse(String(flags.get("raw")));
	} else if (flags.has("file")) {
		const raw = fs.readFileSync(String(flags.get("file")), "utf-8");
		request = JSON.parse(raw);
	} else {
		request = buildRequest(positionals, flags);
	}

	const requiresSession = new Set([
		"pi-get-state",
		"pi-get-messages",
		"pi-get-commands",
		"pi-create",
		"pi-prompt",
		"pi-steer",
		"pi-follow-up",
		"pi-abort",
		"pi-close",
	]);
	const command = positionals[0];
	if (!request || (requiresSession.has(command) && request.session_id === undefined)) {
		usage();
		process.exit(1);
	}

	try {
		const resp = await sendRequest(socketPath, request, flags.has("pretty"));
		if (flags.has("pretty")) {
			console.log(JSON.stringify(resp, null, 2));
			return;
		}
		console.log(JSON.stringify(resp));
	} catch (err) {
		console.error("Runner request failed:", err?.message ?? err);
		process.exit(1);
	}
}

main();
