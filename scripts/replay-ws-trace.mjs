#!/usr/bin/env node
import fs from "node:fs";

function usage() {
	console.log("Usage: bun scripts/replay-ws-trace.mjs <trace.json>");
}

const tracePath = process.argv[2];
if (!tracePath) {
	usage();
	process.exit(1);
}

let raw;
try {
	raw = fs.readFileSync(tracePath, "utf-8");
} catch (err) {
	console.error("Failed to read trace file:", err?.message ?? err);
	process.exit(1);
}

let entries;
try {
	entries = JSON.parse(raw);
} catch (err) {
	console.error("Trace file is not valid JSON:", err?.message ?? err);
	process.exit(1);
}

if (!Array.isArray(entries)) {
	console.error("Trace JSON must be an array of events");
	process.exit(1);
}

const sorted = [...entries].sort((a, b) => (a.ts ?? 0) - (b.ts ?? 0));

const sessionState = new Map();
const warnings = [];

function getState(sessionId) {
	if (!sessionState.has(sessionId)) {
		sessionState.set(sessionId, {
			created: false,
			streaming: false,
			lastEventTs: 0,
		});
	}
	return sessionState.get(sessionId);
}

function summarize(entry) {
	const ts = new Date(entry.ts ?? Date.now()).toISOString();
	const dir = entry.dir ?? "?";
	const ch = entry.channel ?? "?";
	const sid = entry.session_id ?? "-";
	const cmd = entry.cmd ? ` cmd=${entry.cmd}` : "";
	const evt = entry.event ? ` event=${entry.event}` : "";
	return `${ts} ${dir} ${ch} session=${sid}${cmd}${evt}`;
}

for (const entry of sorted) {
	const sid = entry.session_id;
	if (!sid) continue;
	const state = getState(sid);
	state.lastEventTs = entry.ts ?? state.lastEventTs;

	if (entry.dir === "send" && entry.cmd === "session.create") {
		if (state.created) {
			warnings.push(`Duplicate session.create for ${sid} at ${entry.ts}`);
		}
		state.created = true;
	}

	if (entry.dir === "recv" && entry.event === "response" && entry.cmd === "session.create") {
		state.created = true;
	}

	if (entry.dir === "recv") {
		if (entry.event === "stream.message_start") state.streaming = true;
		if (entry.event === "stream.message_end") state.streaming = false;
		if (entry.event === "stream.done") state.streaming = false;
		if (entry.event === "agent.idle") state.streaming = false;
	}

	if (entry.dir === "recv" && entry.event === "response" && entry.cmd === "get_messages") {
		if (state.streaming) {
			warnings.push(`get_messages while streaming for ${sid} at ${entry.ts}`);
		}
	}
}

console.log("Timeline:");
for (const entry of sorted) {
	console.log(summarize(entry));
}

if (warnings.length > 0) {
	console.log("\nWarnings:");
	for (const warning of warnings) {
		console.log(`- ${warning}`);
	}
}
