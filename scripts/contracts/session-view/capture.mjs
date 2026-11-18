// Capture golden frames for the session-view contract from the pinned upstream Pi Durable.
// Usage: bun run capture.mjs <output-dir>
// Writes <output-dir>/scenarios/*.json and <output-dir>/ops-vectors.json.
// Deterministic: fake harness clock, fixed faux model, memory storage, normalized provider id.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import { apply, applyImmutable, PathError, UnsafePathError } from "@earendil-works/chord/delta";
import { createModels } from "@earendil-works/pi-ai/models";
import { fauxAssistantMessage, fauxProvider, fauxToolCall } from "@earendil-works/pi-ai/providers/faux";
import { createRegistry, defineExtension, defineTool, Harness, MemoryStorage } from "@earendil-works/pi-durable";
import { obj, str } from "@earendil-works/pi-schema";
import { CONTRACT, createReferenceHost, UI_KIND, VERSION } from "./reference-host.mjs";

const out = process.argv[2];
if (!out) throw new Error("usage: bun run capture.mjs <output-dir>");
const ctx = BACKGROUND_CONTEXT;
const EPOCH = 1767225600000; // 2026-01-01T00:00:00Z
const SESSION = "ses_fixture";
const PROVIDER_SESSION = "00000000-0000-7000-8000-000000000001";
const here = new URL(".", import.meta.url).pathname;
const pinned = JSON.parse(readFileSync(join(here, "package.json"), "utf8")).dependencies["@earendil-works/pi-durable"];
const installed = JSON.parse(readFileSync(join(here, "node_modules/@earendil-works/pi-durable/package.json"), "utf8")).version;
if (pinned !== installed) throw new Error(`installed pi-durable ${installed} is not the pinned ${pinned}`);
const upstream = { package: "@earendil-works/pi-durable", version: installed };

/** Canonical JSON: sorted keys, no whitespace. Fixtures hold integers only, so number text is stable. */
const canonical = (value) => {
	if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
	if (value !== null && typeof value === "object") {
		return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
	}
	if (typeof value === "number" && !Number.isInteger(value)) throw new Error(`non-integer number in fixture: ${value}`);
	return JSON.stringify(value);
};
const sha = (value) => createHash("sha256").update(canonical(value)).digest("hex");
/**
 * The only run-dependent values: the provider affinity id Durable generates per conversation and
 * tool durations, which upstream measures on the wall clock.
 */
const normalize = (value) => {
	if (Array.isArray(value)) return value.map(normalize);
	if (value !== null && typeof value === "object") {
		const copy = {};
		for (const [key, item] of Object.entries(value)) {
			if (key === "sessionId" && typeof item === "string") copy[key] = PROVIDER_SESSION;
			else if (key === "durationMs" && typeof item === "number") copy[key] = 1;
			else copy[key] = normalize(item);
		}
		return copy;
	}
	return value;
};
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function open({ window, responses, tokensPerSecond, parts = false }) {
	let now = EPOCH;
	const faux = fauxProvider({ api: "faux-fixture", provider: "faux", ...(tokensPerSecond ? { tokensPerSecond, tokenSize: { min: 3, max: 3 } } : {}) });
	const models = createModels();
	models.setProvider(faux.provider);
	const model = faux.getModel();
	const gates = new Map();
	const gate = (name) => {
		if (!gates.has(name)) {
			let release;
			const promise = new Promise((resolve) => (release = resolve));
			gates.set(name, { promise, release });
		}
		return gates.get(name);
	};
	const registry = createRegistry();
	// Revision 2.2: a tool that shows a generative UI part through the host (set once the host exists).
	let showPart = async () => {
		throw new Error("host not ready");
	};
	const showPartTool = () =>
		defineTool({
			name: "show_part",
			description: "Show a generative UI part to the user.",
			parameters: obj({ partId: str() }),
			execute: async (args, api) => {
				await showPart(args.partId, api.callId);
				api.output(`shown:${args.partId}`);
				return {};
			},
		});
	registry.install(defineExtension({
		name: "fixture",
		tools: [
			...(parts ? [showPartTool()] : []),
			defineTool({
				name: "echo",
				description: "Return the given text.",
				parameters: obj({ text: str() }),
				replay: "safe",
				execute: async (args, api) => {
					api.output(`echo:${args.text}`);
					return {};
				},
			}),
			defineTool({
				name: "gate",
				description: "Wait until the fixture releases the named gate.",
				parameters: obj({ name: str() }),
				execute: async (args, _api, context) => {
					const aborted = new Promise((_, reject) => {
						const signal = context.abortSignal;
						if (signal?.aborted) reject(new Error("aborted"));
						signal?.addEventListener("abort", () => reject(new Error("aborted")), { once: true });
					});
					await Promise.race([gate(args.name).promise, aborted]);
					return { output: [{ type: "text", text: `released:${args.name}` }] };
				},
			}),
		],
	}));
	const harness = await Harness.open(new MemoryStorage(), {
		models, registry, now: () => ++now,
		settings: { progress: { partialIntervalMs: 0, outputIntervalMs: 0 }, retry: { enabled: false, maxRetries: 0, baseDelayMs: 1 } },
	}, ctx);
	const conversation = await harness.root(ctx, { agent: { model: { provider: model.provider, modelId: model.id } } });
	faux.setResponses(responses);
	const steps = [];
	const values = new Map(); // attachment -> applied value
	const received = [];
	// Steps of the first connection carry no `conn`; later connections are named.
	const recorder = (conn) => (raw) => {
		const frame = normalize(raw);
		received.push({ ...frame, conn: conn ?? "a" });
		const tag = conn === undefined ? {} : { conn };
		if (frame.type === "view") {
			const next = applyImmutable(values.get(frame.attachment), frame.ops);
			values.set(frame.attachment, next);
			steps.push({ host: frame, ...tag, value_sha256: sha(next) });
		} else steps.push({ host: frame, ...tag });
	};
	const host = createReferenceHost({ harness, sessions: { [SESSION]: conversation }, store: "store_fixture", upstream, emit: recorder(undefined) });
	showPart = (partId, call) => host.append(SESSION, `part-${partId}-shown`, { kind: UI_KIND, data: uiPart(partId, "shown", call) });
	const requester = (connection, conn, prefix) => {
		let counter = 0;
		const tag = conn === undefined ? {} : { conn };
		return async (frame) => {
			const full = { id: frame.id ?? `${prefix}${++counter}`, ...frame };
			steps.push({ client: full, ...tag });
			const result = await connection.request(full);
			if (result.close !== undefined) {
				steps.push({ close: result.close, ...tag });
				return result;
			}
			return normalize(result);
		};
	};
	const request = requester(host, undefined, "c");
	const connect = (conn) => requester(host.connect(recorder(conn)), conn, `${conn}`);
	const until = async (label, predicate) => {
		for (let i = 0; i < 2000; i++) {
			await host.settle();
			if (predicate()) return;
			await sleep(5);
		}
		throw new Error(`timeout waiting for ${label}`);
	};
	const receipt = (requestId, status, conn = "a") => () => received.some((f) => f.type === "receipt" && f.request_id === requestId && f.status === status && f.conn === conn);
	const live = (attachment) => values.get(attachment)?.docs?.["pi.live"] ?? {};
	return { harness, conversation, faux, host, request, connect, until, receipt, live, steps, values, received, gate, window };
}

// Revision 2.2: one generative UI part in the Oqto OpenUI profile's core set (a form with a choice).
const PART_BODY = [
	"root = Stack([card])",
	"card = Card([head, form])",
	'head = CardHeader("Pick a sandbox profile", "For the new workspace")',
	'form = Form("profile", btns, [FormControl("Profile", choice)])',
	'choice = RadioGroup("profile", [RadioItem("Minimal", "Home readable, /tmp writable", "minimal"), RadioItem("Strict", "Workspace only, no network", "strict")], "minimal")',
	'btns = Buttons([Button("Create workspace", @Answer({profile: $profile})), Button("Cancel", @Answer({cancelled: true}), "tertiary")])',
].join("\n");
const PART_FALLBACK = "== Pick a sandbox profile ==\nFor the new workspace\nProfile: Minimal / Strict\n[Create workspace] [Cancel]";
const uiPart = (partId, state, call) => ({
	part: {
		schema: "oqto.openui.part.v1",
		partId,
		profile: { id: "oqto", version: "0.2.2" },
		body: PART_BODY,
		fallback: PART_FALLBACK,
		answer: { schema: { type: "object", properties: { profile: { enum: ["minimal", "strict"] }, cancelled: { type: "boolean" } } }, timeoutSeconds: 600 },
	},
	state,
	origin: { call },
});

const answer = (text) => fauxAssistantMessage(text, { timestamp: EPOCH });
const callTool = (name, args, id) => fauxAssistantMessage([fauxToolCall(name, args, { id })], { stopReason: "toolUse", timestamp: EPOCH });

const scenarios = {
	async "prompt-answer"() {
		const s = await open({ responses: [answer("Hello from the fixture.")], tokensPerSecond: 50 });
		const attached = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 4, max: 8 } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-prompt-1", input: { kind: "prompt", text: "Say hello" } });
		await s.until("answer", s.receipt("req-prompt-1", "done"));
		return { s, attachment: attached.attachment, description: "Attach to an empty Session, prompt, streamed partial answer, settled answer.",
			expect: { entry_kinds: ["pi.user", "pi.system", "pi.assistant"], receipts: { "req-prompt-1": ["placed", "done"] }, errors: {}, idle: true, min_ops: { a: 1 } } };
	},
	async "tool-round"() {
		const s = await open({ responses: [callTool("echo", { text: "ping" }, "call_echo_1"), answer("The tool said ping.")] });
		const attached = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 8, max: 16 } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-tool-1", input: { kind: "prompt", text: "Use the echo tool" } });
		await s.until("answer", s.receipt("req-tool-1", "done"));
		return { s, attachment: attached.attachment, description: "One tool round: tool slot progress in pi.live, tool result entry, final answer.",
			expect: { entry_kinds: ["pi.user", "pi.system", "pi.assistant", "pi.tool-result", "pi.assistant"], receipts: { "req-tool-1": ["placed", "done"] }, errors: {}, idle: true } };
	},
	async "busy-inputs"() {
		const s = await open({ responses: [callTool("gate", { name: "g1" }, "call_gate_1"), answer("First answer."), answer("Second answer."), answer("Third answer.")] });
		const attached = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 12, max: 24 } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-busy-1", input: { kind: "prompt", text: "Wait at the gate" } });
		await s.until("gate running", () => (s.live(attached.attachment).tools ?? []).some((slot) => slot.status === "running"));
		await s.request({ type: "submit", session: SESSION, request_id: "req-busy-2", input: { kind: "prompt", text: "Second prompt while busy" } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-busy-3", input: { kind: "steer", text: "Also mention the gate" } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-busy-4", input: { kind: "follow_up", text: "Then answer again" } });
		s.gate("g1").release();
		await s.until("follow-up answered", s.receipt("req-busy-4", "done"));
		await s.request({ type: "submit", session: SESSION, request_id: "req-busy-5", input: { kind: "steer", text: "Steer with nothing running" } });
		// A refusal had no effect, so the same request_id is free for the retry.
		await s.request({ type: "submit", session: SESSION, request_id: "req-busy-5", input: { kind: "prompt", text: "Steer with nothing running" } });
		await s.until("retry answered", s.receipt("req-busy-5", "done"));
		return { s, attachment: attached.attachment, description: "While a run is busy: prompt is refused, steer joins the run, follow_up starts the next run; steer when idle is refused, and the refused request_id is reused for a prompt.",
			expect: { entry_kinds: ["pi.user", "pi.system", "pi.assistant", "pi.tool-result", "pi.user", "pi.assistant", "pi.user", "pi.assistant", "pi.user", "pi.assistant"],
				receipts: { "req-busy-1": ["placed", "done"], "req-busy-3": ["queued", "placed", "done"], "req-busy-4": ["queued", "placed", "done"], "req-busy-5": ["placed", "done"] },
				errors: { "req-busy-2": "busy", "req-busy-5": "idle" }, idle: true } };
	},
	async abort() {
		const s = await open({ responses: [callTool("gate", { name: "never" }, "call_gate_2")] });
		const attached = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 8, max: 16 } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-abort-1", input: { kind: "prompt", text: "Wait forever" } });
		await s.until("gate running", () => (s.live(attached.attachment).tools ?? []).some((slot) => slot.status === "running"));
		await s.request({ type: "submit", session: SESSION, request_id: "req-abort-queued", input: { kind: "follow_up", text: "Never runs" } });
		await s.request({ type: "abort", session: SESSION, request_id: "req-abort-2" });
		await s.until("queued input withdrawn", s.receipt("req-abort-queued", "unanswered"));
		await s.until("prompt unanswered", s.receipt("req-abort-1", "unanswered"));
		return { s, attachment: attached.attachment, description: "Abort a running tool round with one input queued: the prompt settles unanswered with reason aborted, the queued input is withdrawn and never runs, pi.live.runFailure says aborted.",
			expect: { entry_kinds: ["pi.user", "pi.system", "pi.assistant", "pi.tool-result"], receipts: { "req-abort-1": ["placed", "unanswered"], "req-abort-queued": ["queued", "unanswered"], "req-abort-2": ["done"] }, errors: {}, idle: true, run_failure: "aborted" } };
	},
	async "passive-write"() {
		const s = await open({ responses: [] });
		const attached = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 4, max: 8 } });
		await s.request({ type: "write", session: SESSION, request_id: "req-write-1", entry: { kind: "app.note", data: { text: "the user opened a file" } } });
		await s.until("write done", s.receipt("req-write-1", "done"));
		await s.request({ type: "write", session: SESSION, request_id: "req-write-2", entry: { kind: "pi.user", data: { text: "not allowed" } } });
		if (s.faux.state.callCount !== 0) throw new Error("a passive write reached the model");
		return { s, attachment: attached.attachment, description: "A passive write appends an entry and never starts a run; a pi.* kind is refused.",
			expect: { entry_kinds: ["app.note"], receipts: { "req-write-1": ["done"] }, errors: { "req-write-2": "invalid" }, idle: true, never_ran: true } };
	},
	async "window-resume"() {
		const s = await open({ responses: [answer("One."), answer("Two."), answer("Three.")] });
		const first = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 2, max: 4 } });
		for (const n of [1, 2, 3]) {
			await s.request({ type: "submit", session: SESSION, request_id: `req-window-${n}`, input: { kind: "prompt", text: `Prompt ${n}` } });
			await s.until(`answer ${n}`, s.receipt(`req-window-${n}`, "done"));
		}
		const held = s.values.get(first.attachment);
		const newest = held.entries[held.entries.length - 1].id;
		await s.request({ type: "detach", attachment: first.attachment });
		s.faux.appendResponses([answer("Four.")]);
		await s.request({ type: "submit", session: SESSION, request_id: "req-window-4", input: { kind: "prompt", text: "Prompt 4 while detached" } });
		await s.until("answer 4", s.receipt("req-window-4", "done"));
		const second = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 2, max: 4, from: newest + 1 } });
		await s.until("resumed base", () => s.values.has(second.attachment));
		const resumed = s.values.get(second.attachment);
		if (resumed.entries.length !== 2 || resumed.before !== newest) throw new Error("resume did not return exactly the missed entries");
		return { s, attachment: second.attachment, description: "The window drops its oldest entries in one splice past max; a detached client re-attaches with from = newest held + 1 and gets only what it missed.",
			expect: { entry_kinds: ["pi.user", "pi.assistant"], receipts: { "req-window-4": ["placed", "done"] }, errors: {}, idle: true, before: newest, splice_seen: true } };
	},
	async "fill-gap"() {
		const s = await open({ responses: [1, 2, 3, 4, 5].map((n) => answer(`Answer ${n}.`)) });
		const first = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 2, max: 4 } });
		for (const n of [1, 2]) {
			await s.request({ type: "submit", session: SESSION, request_id: `req-gap-${n}`, input: { kind: "prompt", text: `Prompt ${n}` } });
			await s.until(`answer ${n}`, s.receipt(`req-gap-${n}`, "done"));
		}
		const cached = s.values.get(first.attachment);
		const newest = cached.entries[cached.entries.length - 1].id;
		await s.request({ type: "detach", attachment: first.attachment });
		for (const n of [3, 4, 5]) {
			await s.request({ type: "submit", session: SESSION, request_id: `req-gap-${n}`, input: { kind: "prompt", text: `Prompt ${n} while away` } });
			await s.until(`answer ${n}`, s.receipt(`req-gap-${n}`, "done"));
		}
		const second = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 2, max: 4, from: newest + 1 } });
		await s.until("resumed base", () => s.values.has(second.attachment));
		const window = s.values.get(second.attachment);
		if (window.before === newest) throw new Error("expected a gap: more was missed than fits the window");
		// Fill exactly the gap: after the newest cached entry, up to and including `before`.
		let page = await s.request({ type: "read_entries", session: SESSION, min: newest + 1, max: window.before, order: "ascending", limit: 3 });
		let pages = 1;
		while (page.next !== undefined) {
			page = await s.request({ type: "read_entries", session: SESSION, min: newest + 1, max: window.before, order: "ascending", limit: 3, cursor: page.next });
			pages++;
		}
		if (pages < 2) throw new Error("expected the gap to need more than one page");
		return { s, attachment: second.attachment, description: "A client that cached a window misses more than fits a new window: the new window starts later (before is past its newest), and paged reads fill exactly the entries in between, so nothing is thrown away.",
			expect: { gap: true, covers_transcript: true, pages: pages, idle: true, errors: {} } };
	},
	async "load-earlier"() {
		const s = await open({ responses: [1, 2, 3].map((n) => answer(`Answer ${n}.`)) });
		// History written before this client attaches (by another client, or earlier).
		for (const n of [1, 2, 3]) {
			await s.request({ type: "submit", session: SESSION, request_id: `req-earlier-${n}`, input: { kind: "prompt", text: `Earlier prompt ${n}` } });
			await s.until(`earlier answer ${n}`, s.receipt(`req-earlier-${n}`, "done"));
		}
		const attached = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 2, max: 4 } });
		await s.until("base", () => s.values.has(attached.attachment));
		const window = s.values.get(attached.attachment);
		if (window.before === undefined) throw new Error("expected earlier entries before the window");
		let page = await s.request({ type: "read_entries", session: SESSION, max: window.before, order: "descending", limit: 2 });
		let pages = 1;
		while (page.next !== undefined) {
			page = await s.request({ type: "read_entries", session: SESSION, max: window.before, order: "descending", limit: 2, cursor: page.next });
			pages++;
		}
		const refused = [
			await s.request({ type: "read_entries", session: SESSION, order: "sideways" }),
			await s.request({ type: "read_entries", session: SESSION, limit: 500 }),
			await s.request({ type: "read_entries", session: SESSION, cursor: "not-a-cursor" }),
		];
		return { s, attachment: attached.attachment, description: "Attach with a small window to a Session that already has history, then load earlier entries page by page, newest first, until nothing is left; malformed reads are refused.",
			expect: { gap: false, covers_transcript: true, pages: pages, idle: true, errors: Object.fromEntries(refused.map((r) => [r.id, r.code])) } };
	},
	async "two-clients"() {
		const s = await open({ responses: [callTool("gate", { name: "g1" }, "call_gate_1"), answer("Done, and I saw the steer.")] });
		const b = s.connect("b");
		const a = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 8, max: 16 } });
		const attachedB = await b({ type: "attach", v: VERSION, session: SESSION, window: { min: 8, max: 16 } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-a-1", input: { kind: "prompt", text: "Wait at the gate" } });
		await s.until("b sees the run", () => (s.live(attachedB.attachment).tools ?? []).some((slot) => slot.status === "running"));
		await b({ type: "submit", session: SESSION, request_id: "req-b-1", input: { kind: "steer", text: "Mention me too" } });
		s.gate("g1").release();
		await s.until("a's prompt done on b", s.receipt("req-a-1", "done", "b"));
		await s.until("b's steer done on a", s.receipt("req-b-1", "done", "a"));
		// A resend changes nothing and is answered only on the connection that sent it.
		await s.request({ type: "submit", session: SESSION, request_id: "req-a-1", input: { kind: "prompt", text: "Wait at the gate" } });
		// A reused frame id cannot be answered unambiguously: the host closes that connection.
		await b({ id: "b1", type: "detach", attachment: attachedB.attachment });
		return { s, attachment: a.attachment, description: "Two connections attached to one Session: each sees the other's requests settle, after the entries they name; a resend is answered only to its sender; a reused frame id closes that connection.",
			expect: { idle: true, errors: {}, receipts_by_conn: { a: { "req-a-1": ["placed", "done", "done"], "req-b-1": ["queued", "placed", "done"] }, b: { "req-a-1": ["placed", "done"], "req-b-1": ["queued", "placed", "done"] } }, closed: ["b"] } };
	},
	async "exactly-once"() {
		const s = await open({ responses: [answer("Only once.")] });
		const attached = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 4, max: 8 } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-once-1", input: { kind: "prompt", text: "Do it once" } });
		await s.until("answer", s.receipt("req-once-1", "done"));
		await s.request({ type: "submit", session: SESSION, request_id: "req-once-1", input: { kind: "prompt", text: "Do it once" } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-once-1", input: { kind: "prompt", text: "Different text" } });
		await s.request({ type: "receipt_query", session: SESSION, request_id: "req-once-1" });
		await s.request({ type: "receipt_query", session: SESSION, request_id: "req-unknown" });
		await s.request({ type: "attach", v: 7, session: SESSION });
		await s.request({ type: "attach", v: VERSION, session: "ses_other" });
		if (s.faux.state.callCount !== 1) throw new Error("a repeated request_id reached the model again");
		return { s, attachment: attached.attachment, description: "A repeated request_id returns the same receipt without a second run; changed content under the same request_id, unknown requests, versions and Sessions are refused.",
			expect: { entry_kinds: ["pi.user", "pi.system", "pi.assistant"], receipts: { "req-once-1": ["placed", "done", "done", "done"] },
				errors: { "c4": "request_conflict", "req-unknown": "unknown_request", "c7": "unsupported_version", "c8": "unknown_session" }, idle: true, assistant_entries: 1 } };
	},
	async "ui-part"() {
		const s = await open({ parts: true, responses: [callTool("show_part", { partId: "part_profile_1" }, "call_part_1"), answer("Pick a profile in the form above.")] });
		const attached = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 4, max: 8 } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-part-1", input: { kind: "prompt", text: "Set up the workspace" } });
		await s.until("answer", s.receipt("req-part-1", "done"));
		// The host takes the part back: a later state of the same part is a new entry.
		await s.host.append(SESSION, "part-part_profile_1-withdrawn", { kind: UI_KIND, data: uiPart("part_profile_1", "withdrawn", "call_part_1") });
		await s.until("withdrawn", () => (s.values.get(attached.attachment)?.entries ?? []).filter((e) => e.kind === UI_KIND).length === 2);
		// A client cannot forge a part.
		await s.request({ type: "write", session: SESSION, request_id: "req-forge-1", entry: { kind: UI_KIND, data: uiPart("part_forged_1", "shown", "call_forged") } });
		return { s, attachment: attached.attachment, description: "Revision 2.2: a tool shows a generative UI part as an oqto.ui entry written by the host; a later state of the same part is a new entry with the same partId; a client write of kind oqto.ui is refused.",
			expect: { entry_kinds: ["pi.user", "pi.system", "pi.assistant", "oqto.ui", "pi.tool-result", "pi.assistant", "oqto.ui"], receipts: { "req-part-1": ["placed", "done"] }, errors: { "req-forge-1": "invalid" }, idle: true } };
	},
	async "model-error"() {
		const s = await open({ responses: [fauxAssistantMessage("", { stopReason: "error", errorMessage: "fixture provider failure", timestamp: EPOCH })] });
		const attached = await s.request({ type: "attach", v: VERSION, session: SESSION, window: { min: 4, max: 8 } });
		await s.request({ type: "submit", session: SESSION, request_id: "req-error-1", input: { kind: "prompt", text: "This will fail" } });
		await s.until("unanswered", s.receipt("req-error-1", "unanswered"));
		return { s, attachment: attached.attachment, description: "The model fails: the prompt settles unanswered and pi.live.runFailure keeps the reason after the run ended.",
			expect: { entry_kinds: ["pi.user", "pi.system", "pi.assistant"], receipts: { "req-error-1": ["placed", "unanswered"] }, errors: {}, idle: true, run_failure: "model_error" } };
	},
};

mkdirSync(join(out, "scenarios"), { recursive: true });
for (const [name, run] of Object.entries(scenarios)) {
	const { s, attachment, description, expect } = await run();
	await s.host.settle();
	await sleep(20);
	const final = s.values.get(attachment);
	// The contract's core invariant, checked against upstream itself: applying the frames a
	// client received gives the view upstream holds for the same window.
	const direct = await s.harness.observeConversation(s.conversation.id, { advance() {}, end() {} }, ctx, { from: final.entries[0]?.id ?? (final.before ?? 0) + 1, min: 200, max: 400 });
	direct.detach();
	const truth = normalize(JSON.parse(JSON.stringify(direct.value)));
	if (canonical(truth.docs) !== canonical(final.docs)) throw new Error(`${name}: applied docs differ from upstream`);
	if (canonical(truth.entries) !== canonical(final.entries)) throw new Error(`${name}: applied entries differ from upstream`);
	// Everything the Session holds, oldest first: what a client that cached, resumed and paged
	// must have in total, and the reference that cached entries never differ from.
	const transcript = [];
	for (let cursor, page; (page = await s.conversation.entries({ order: "ascending" }, 200, cursor, ctx)); ) {
		transcript.push(...normalize(JSON.parse(JSON.stringify(page.items))));
		if (page.next === undefined) break;
		cursor = page.next;
	}
	s.host.closing();
	await s.harness.close(ctx);
	const scenario = { contract: CONTRACT, v: VERSION, scenario: name, description, upstream, session: SESSION, attachment, expect, steps: s.steps, final_value: final, full_transcript: transcript };
	writeFileSync(join(out, "scenarios", `${name}.json`), `${JSON.stringify(scenario, null, 1)}\n`);
	console.log(`${name}: ${s.steps.length} steps, ${s.steps.filter((step) => step.host?.type === "view").length} view frames`);
}

// Op vectors from upstream Chord's own applier: what any client applier must reproduce.
const vector = (name, value, ops) => {
	try {
		return { name, value, ops, result: apply(JSON.parse(JSON.stringify(value)), JSON.parse(JSON.stringify(ops))) };
	} catch (cause) {
		const error = cause instanceof UnsafePathError ? "unsafe_path" : cause instanceof PathError ? "unresolvable_path" : cause instanceof TypeError ? "invalid_op" : undefined;
		if (error === undefined) throw cause;
		return { name, value, ops, error };
	}
};
const vectors = [
	vector("r replaces the root", { a: 1 }, [["r", { b: 2 }]]),
	vector("r then further ops in one batch", null, [["r", { list: [] }], ["p", ["list"], 0, 0, [1, 2]], ["s", ["n"], 3]]),
	vector("s sets an object key", { a: 1 }, [["s", ["b"], { c: [1] }]]),
	vector("s replaces an array element", { xs: [1, 2, 3] }, [["s", ["xs", 1], "two"]]),
	vector("s appends exactly at the array end", { xs: [1, 2] }, [["s", ["xs", 2], 3]]),
	vector("s past the array end is refused", { xs: [1, 2] }, [["s", ["xs", 4], 3]]),
	vector("s through a missing parent is refused", { a: {} }, [["s", ["a", "b", "c"], 1]]),
	vector("s with an empty path is refused", { a: 1 }, [["s", [], 2]]),
	vector("d deletes an object key", { a: 1, b: 2 }, [["d", ["a"]]]),
	vector("d removes an array element and shifts the rest", { xs: ["a", "b", "c"] }, [["d", ["xs", 0]]]),
	vector("d past the array end is refused", { xs: ["a"] }, [["d", ["xs", 3]]]),
	vector("d exactly at the array end", { xs: ["a"] }, [["d", ["xs", 1]]]),
	vector("d on a missing object key", { a: 1 }, [["d", ["b"]]]),
	vector("d through a missing parent is refused", { a: 1 }, [["d", ["b", "c"]]]),
	vector("a appends text", { t: "Hello" }, [["a", ["t"], ", world"]]),
	vector("a on a non-string is refused", { t: 5 }, [["a", ["t"], "x"]]),
	vector("a on a missing key is refused", {}, [["a", ["t"], "x"]]),
	vector("t drops leading UTF-16 code units", { t: "abcdef" }, [["t", ["t"], 2]]),
	vector("t counts UTF-16 code units, not characters", { t: "\u{1F600}ab" }, [["t", ["t"], 2]]),
	vector("t beyond the length leaves an empty string", { t: "ab" }, [["t", ["t"], 5]]),
	vector("p inserts", { xs: [1, 4] }, [["p", ["xs"], 1, 0, [2, 3]]]),
	vector("p removes", { xs: [1, 2, 3, 4] }, [["p", ["xs"], 1, 2, []]]),
	vector("p replaces", { xs: [1, 2, 3] }, [["p", ["xs"], 0, 2, ["a"]]]),
	vector("p on the root array", [1, 2, 3], [["p", [], 0, 1, []]]),
	vector("p with an index past the array end", { xs: [1] }, [["p", ["xs"], 5, 0, [2]]]),
	vector("p removing more than remain", { xs: [1, 2, 3] }, [["p", ["xs"], 1, 9, ["z"]]]),
	vector("p on a non-array is refused", { xs: {} }, [["p", ["xs"], 0, 0, [1]]]),
	vector("m reorders: new[i] = old[permutation[i]]", { xs: ["a", "b", "c"] }, [["m", ["xs"], [2, 0, 1]]]),
	vector("m with a wrong length is refused", { xs: ["a", "b", "c"] }, [["m", ["xs"], [1, 0]]]),
	vector("m that is not a permutation is refused", { xs: ["a", "b"] }, [["m", ["xs"], [0, 0]]]),
	vector("reserved segment __proto__ is refused", { a: {} }, [["s", ["a", "__proto__", "x"], 1]]),
	vector("reserved segment constructor is refused", { a: {} }, [["s", ["constructor"], 1]]),
	vector("negative index is refused", { xs: [1] }, [["s", ["xs", -1], 2]]),
	vector("string index into an array is refused", { xs: [1] }, [["s", ["xs", "0"], 2]]),
	vector("unknown verb is refused", { a: 1 }, [["x", ["a"], 2]]),
	vector("wrong arity is refused", { a: 1 }, [["s", ["a"]]]),
	vector("path that is not an array is refused", { a: 1 }, [["s", "a", 2]]),
	vector("ops apply in order", { log: "", xs: [] }, [["a", ["log"], "one"], ["p", ["xs"], 0, 0, ["x"]], ["a", ["log"], ",two"], ["d", ["xs", 0]]]),
];
writeFileSync(join(out, "ops-vectors.json"), `${JSON.stringify({ contract: CONTRACT, v: VERSION, source: { package: "@earendil-works/chord", version: JSON.parse(readFileSync(join(here, "node_modules/@earendil-works/chord/package.json"), "utf8")).version }, vectors }, null, 1)}\n`);
console.log(`ops-vectors: ${vectors.length} (${vectors.filter((v) => v.error).length} refusals)`);
process.exit(0);
